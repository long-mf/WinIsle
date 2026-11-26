//! 实时音频分析：整体电平 + 分频段能量（供水波频谱真实随音乐起伏）。
//!
//! - [`level`]：通过 WASAPI `IAudioMeterInformation::GetPeakValue` 读取系统整体峰值（0~1）。
//! - [`bands`]：通过 WASAPI **环回采集（loopback）** 抓默认渲染端点的混音流，做 1024 点 FFT，
//!   折算成 [`NBANDS`] 个对数频段的相对能量（0~1，带自适应增益 + 平滑）。
//!
//! 两个后台线程都只写原子变量，绘制线程直接读取；拿不到数据时返回 0。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static LEVEL: AtomicU32 = AtomicU32::new(0);

/// 频段数量（对数分布，低→高）。
pub const NBANDS: usize = 64;
const ZERO: AtomicU32 = AtomicU32::new(0);
static BANDS: [AtomicU32; NBANDS] = [ZERO; NBANDS];
/// 低频“鼓点”包络（0~1，快起快落），用于让整体柱子跟着鼓点弹跳。
static KICK: AtomicU32 = AtomicU32::new(0);

/// UI 每帧上报「当前是否应有声音」（音乐正在播放）。采集线程据此判定
/// 「本应有声音却长时间收不到环回数据包」= 采集卡死 → 自我重建；真正暂停时不触发，保持空闲省电。
static EXPECT_AUDIO: AtomicBool = AtomicBool::new(false);

/// 由主线程每帧调用：告知音频线程当前是否处于“应该在出声”的状态。
pub fn set_expected_playing(playing: bool) {
    EXPECT_AUDIO.store(playing, Ordering::Relaxed);
}

/// 当前音量峰值（0.0~1.0，已做轻度平滑）。
pub fn level() -> f32 {
    f32::from_bits(LEVEL.load(Ordering::Relaxed)).clamp(0.0, 1.0)
}

/// 低频鼓点包络（快攻击、快释放），0~1。
pub fn kick() -> f32 {
    f32::from_bits(KICK.load(Ordering::Relaxed)).clamp(0.0, 1.0)
}

/// 当前各频段相对能量（0.0~1.0，低→高，已自适应归一 + 平滑）。
pub fn bands() -> [f32; NBANDS] {
    let mut out = [0f32; NBANDS];
    for (i, slot) in BANDS.iter().enumerate() {
        out[i] = f32::from_bits(slot.load(Ordering::Relaxed)).clamp(0.0, 1.0);
    }
    out
}

/// 启动后台采样线程（进程内只调一次）。
pub fn spawn() {
    // 1) 整体电平线程
    std::thread::Builder::new()
        .name("winisle-audio-meter".into())
        .spawn(|| unsafe {
            use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
            use windows::Win32::Media::Audio::{
                eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
            };
            use windows::Win32::System::Com::{
                CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
            };

            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let mut meter: Option<IAudioMeterInformation> = None;
            let mut backoff = 0u32;
            loop {
                if meter.is_none() {
                    meter = (|| {
                        let en: IMMDeviceEnumerator =
                            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
                        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
                        dev.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None)
                            .ok()
                    })();
                    if meter.is_none() {
                        LEVEL.store(0f32.to_bits(), Ordering::Relaxed);
                    }
                }
                if let Some(m) = &meter {
                    match m.GetPeakValue() {
                        Ok(v) => {
                            let prev = f32::from_bits(LEVEL.load(Ordering::Relaxed));
                            let target = v.clamp(0.0, 1.0);
                            let alpha = if target > prev { 0.55 } else { 0.20 };
                            let nv = prev + (target - prev) * alpha;
                            LEVEL.store(nv.to_bits(), Ordering::Relaxed);
                            backoff = 0;
                        }
                        Err(_) => {
                            meter = None;
                        }
                    }
                }
                if meter.is_none() {
                    backoff = backoff.saturating_add(1);
                    if backoff % 60 != 0 {
                        std::thread::sleep(std::time::Duration::from_millis(33));
                        continue;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
        })
        .ok();

    // 2) 分频段能量线程
    std::thread::Builder::new()
        .name("winisle-audio-bands".into())
        .spawn(|| unsafe { bands_loop() })
        .ok();
}

const NFFT: usize = 1024;
const PI: f32 = std::f32::consts::PI;

/// 原地基 2 FFT（要求长度 2 的幂）。
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    // 位反转
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0usize;
        while i < n {
            let mut cur_r = 1.0f32;
            let mut cur_i = 0.0f32;
            for k in 0..len / 2 {
                let ur = re[i + k];
                let ui = im[i + k];
                let vr = re[i + k + len / 2] * cur_r - im[i + k + len / 2] * cur_i;
                let vi = re[i + k + len / 2] * cur_i + im[i + k + len / 2] * cur_r;
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let nr = cur_r * wr - cur_i * wi;
                cur_i = cur_r * wi + cur_i * wr;
                cur_r = nr;
            }
            i += len;
        }
        len <<= 1;
    }
}

unsafe fn bands_loop() {
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

    let mut hann = [0f32; NFFT];
    for (i, h) in hann.iter_mut().enumerate() {
        *h = 0.5 - 0.5 * (2.0 * PI * i as f32 / (NFFT as f32 - 1.0)).cos();
    }

    let mut smooth = [0f32; NBANDS];
    let mut avg = [0.02f32; NBANDS];
    let mut kick_env = 0f32;
    let mut last_lowmax = 0f32;
    let mut mono: Vec<f32> = Vec::with_capacity(NFFT * 4);

    'reconnect: loop {
        // ---- 建立环回采集 ----
        let setup = (|| -> windows::core::Result<(IAudioClient, IAudioCaptureClient, u32, u16, u16)> {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
            let pfmt = client.GetMixFormat()?;
            let wfx = *pfmt;
            let init = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                0, // shared 模式交给引擎默认周期（10ms 级），比手动 2s 缓冲延迟小得多
                0,
                pfmt,
                None,
            );
            CoTaskMemFree(Some(pfmt as *const core::ffi::c_void));
            init?;
            let cap: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok((client, cap, wfx.nSamplesPerSec, wfx.nChannels, wfx.wBitsPerSample))
        })();

        let (client, cap, sr, ch, bits) = match setup {
            Ok(v) => v,
            Err(_) => {
                for b in BANDS.iter() {
                    b.store(0f32.to_bits(), Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(1000));
                continue;
            }
        };
        let ch = ch.max(1) as usize;
        // 本连接最后一次成功收到数据包的时刻（用于卡死自愈判定）。
        let mut last_pkt = std::time::Instant::now();

        // 频段边界（对数，低→高），随采样率确定
        let fmin = 40f32;
        let fmax = (sr as f32 * 0.5 * 0.95).min(16000.0).max(fmin * 2.0);
        let mut blo = [0usize; NBANDS];
        let mut bhi = [0usize; NBANDS];
        for b in 0..NBANDS {
            let t0 = b as f32 / NBANDS as f32;
            let t1 = (b + 1) as f32 / NBANDS as f32;
            let lo = fmin * (fmax / fmin).powf(t0);
            let hi = fmin * (fmax / fmin).powf(t1);
            let mut k0 = (lo * NFFT as f32 / sr as f32).floor() as usize;
            let mut k1 = (hi * NFFT as f32 / sr as f32).ceil() as usize;
            k0 = k0.clamp(1, NFFT / 2 - 1);
            k1 = k1.clamp(k0, NFFT / 2 - 1);
            blo[b] = k0;
            bhi[b] = k1;
        }

        // ---- 采集循环 ----
        // 每轮把所有可用包全部排空再睡：环回在 shared 模式下以很小的时间片供包，若每轮只取一包就 sleep，
        // 包会越积越多 → 频谱相对声音越来越滞后（“反应延迟 / 对不上鼓点”）。排空后延迟≈一个引擎周期。
        let mut last_t = std::time::Instant::now();
        loop {
            let mut got_any = false;
            let mut reconnect = false;
            let mut drained = 0u32;
            loop {
                let pkt: u32 = match cap.GetNextPacketSize() {
                    Ok(p) => p,
                    Err(_) => {
                        reconnect = true;
                        break;
                    }
                };
                if pkt == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
                match cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None) {
                    Ok(()) => {}
                    Err(_) => {
                        reconnect = true;
                        break;
                    }
                }
                if frames > 0 {
                    let n = frames as usize * ch;
                    if data.is_null() {
                        for _ in 0..frames {
                            mono.push(0.0);
                        }
                    } else if bits == 32 {
                        let s = std::slice::from_raw_parts(data as *const f32, n);
                        for f in 0..frames as usize {
                            let mut acc = 0f32;
                            for c in 0..ch {
                                acc += s[f * ch + c];
                            }
                            mono.push(acc / ch as f32);
                        }
                    } else if bits == 16 {
                        let s = std::slice::from_raw_parts(data as *const i16, n);
                        for f in 0..frames as usize {
                            let mut acc = 0f32;
                            for c in 0..ch {
                                acc += s[f * ch + c] as f32 / 32768.0;
                            }
                            mono.push(acc / ch as f32);
                        }
                    } else {
                        for _ in 0..frames {
                            mono.push(0.0);
                        }
                    }
                    let _ = cap.ReleaseBuffer(frames);
                    got_any = true;
                } else {
                    let _ = cap.ReleaseBuffer(frames);
                    break;
                }
                drained += 1;
                if drained > 2048 {
                    break; // 安全上限，防异常时死循环
                }
            }
            if reconnect {
                break; // 设备变化/流断开 → 重连
            }

            let now = std::time::Instant::now();
            let dt = (now - last_t).as_secs_f32().clamp(0.001, 0.10);
            last_t = now;
            if got_any {
                last_pkt = now;
            }
            let starved = now.duration_since(last_pkt).as_secs_f32();

            if got_any && mono.len() > NFFT * 4 {
                let drop = mono.len() - NFFT * 2;
                mono.drain(0..drop);
            }

            if got_any && mono.len() >= NFFT {
                let mut re = [0f32; NFFT];
                let mut im = [0f32; NFFT];
                let start = mono.len() - NFFT;
                for i in 0..NFFT {
                    re[i] = mono[start + i] * hann[i];
                }
                fft(&mut re, &mut im);
                // 时间常数化的平滑（与帧率无关，避免改了采集节奏后手感漂移）
                let a_up = 1.0 - (-dt / 0.02_f32).exp();
                let a_dn = 1.0 - (-dt / 0.12_f32).exp();
                let a_avg = 1.0 - (-dt / 0.50_f32).exp();
                let mut lowmax = 0f32;
                for b in 0..NBANDS {
                    let mut sum = 0f32;
                    let mut cnt = 0u32;
                    for k in blo[b]..=bhi[b] {
                        sum += (re[k] * re[k] + im[k] * im[k]).sqrt();
                        cnt += 1;
                    }
                    let e = if cnt > 0 { sum / cnt as f32 } else { 0.0 } / (NFFT as f32 * 0.5);
                    // 上升快、回落稍慢 → 鼓点有弹跳感又不过度抖动
                    let a = if e > smooth[b] { a_up } else { a_dn };
                    smooth[b] += (e - smooth[b]) * a;
                    // 缓慢基线做动态范围压缩：瞬态鼓点相对基线被显著拉高，静音迅速回落
                    avg[b] += (smooth[b] - avg[b]) * a_avg;
                    let val = (smooth[b] / (avg[b] * 2.2 + 0.003)).clamp(0.0, 1.0);
                    let val = val.powf(0.8);
                    if b <= 10 {
                        lowmax = lowmax.max(val);
                    }
                    BANDS[b].store(val.to_bits(), Ordering::Relaxed);
                }
                // 低频鼓点包络：只看“上升沿/起振量”，而不是低频的绝对能量。
                // 持续的低音铺底（bass line）不会一直被当成鼓点；每次底鼓把低频推高时才触发一次快起快落。
                let rise = (lowmax - last_lowmax).max(0.0);
                last_lowmax = lowmax;
                let kick_target = (rise * 6.0).clamp(0.0, 1.0);
                let k_up = 1.0 - (-dt / 0.006_f32).exp();
                let k_dn = 1.0 - (-dt / 0.08_f32).exp();
                let ka = if kick_target > kick_env { k_up } else { k_dn };
                kick_env += (kick_target - kick_env) * ka;
                KICK.store(kick_env.to_bits(), Ordering::Relaxed);
            } else if starved > 0.06 {
                // 没有新音频数据（暂停 / 端点空闲 / 采集暂时无包）：让频谱自然回落到 0，
                // 而不是拿 mono 里的旧样本反复算 FFT、把频谱冻结在最后一帧的值上。
                mono.clear(); // 丢弃陈旧样本，避免恢复播放瞬间把旧样本拼进第一帧 FFT
                let a_sil = 1.0 - (-dt / 0.12_f32).exp();
                for b in 0..NBANDS {
                    smooth[b] -= smooth[b] * a_sil;
                    avg[b] += (smooth[b] - avg[b]) * a_sil;
                    let val = (smooth[b] / (avg[b] * 2.2 + 0.003)).clamp(0.0, 1.0).powf(0.8);
                    BANDS[b].store(val.to_bits(), Ordering::Relaxed);
                }
                let k_sil = 1.0 - (-dt / 0.08_f32).exp();
                kick_env -= kick_env * k_sil;
                KICK.store(kick_env.to_bits(), Ordering::Relaxed);
            }

            // 卡死自愈：环回采集在渲染端点长时间静默/空闲后可能不再吐包（GetNextPacketSize 恒 0，
            // 不返回错误，也不会走 reconnect 分支），若只等新包就会永久冻结。此时 UI 仍在正常判定
            // is_playing=true。故：本应有声音却长时间收不到包，或峰值电平明显有声音却收不到包，
            // 就主动重建采集客户端（Stop→重新 Initialize/Start），避免只能重启整个程序。
            // 真静音时 expect=false 且电平≈0，不触发重建、不耗电。
            let expect_audio = EXPECT_AUDIO.load(Ordering::Relaxed);
            let audible = level() > 0.03;
            if (starved > 0.8 && expect_audio) || (starved > 3.0 && audible) {
                reconnect = true;
            }
            if reconnect {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let _ = client.Stop();
        for b in BANDS.iter() {
            b.store(0f32.to_bits(), Ordering::Relaxed);
        }
        KICK.store(0f32.to_bits(), Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(600));
        continue 'reconnect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 手动探针：需系统正在播放声音。`cargo test --release probe_peak -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_peak() {
        spawn();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let mut mx = 0.0f32;
        for _ in 0..12 {
            let v = level();
            mx = mx.max(v);
            println!("level={:.3}", v);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert!(mx > 0.05, "no audio peak detected (music playing?)");
    }

    /// 手动探针：打印频段能量。`cargo test --release probe_bands -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_bands() {
        spawn();
        std::thread::sleep(std::time::Duration::from_millis(800));
        for _ in 0..6 {
            let b = bands();
            let s: Vec<String> = b.iter().map(|v| format!("{:.2}", v)).collect();
            println!("bands=[{}]", s.join(" "));
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
}
