//! Métricas do sistema para mostrar na tela: temperaturas (hwmon) e uso de
//! CPU e RAM (`/proc`). Lê arquivos do kernel diretamente, o que também
//! funciona dentro do Flatpak.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Uma métrica que pode aparecer na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Metric {
    CpuTemp,
    CpuUsage,
    GpuTemp,
    RamUsage,
}

impl Metric {
    pub const ALL: [Metric; 4] = [
        Metric::CpuTemp,
        Metric::CpuUsage,
        Metric::GpuTemp,
        Metric::RamUsage,
    ];

    /// Rótulo curto, desenhado na tela.
    pub fn label(self) -> &'static str {
        match self {
            Metric::CpuTemp => "CPU",
            Metric::CpuUsage => "CPU LOAD",
            Metric::GpuTemp => "GPU",
            Metric::RamUsage => "RAM",
        }
    }

    /// Descrição para a interface.
    pub fn description(self) -> &'static str {
        match self {
            Metric::CpuTemp => "CPU temperature",
            Metric::CpuUsage => "CPU usage",
            Metric::GpuTemp => "GPU temperature",
            Metric::RamUsage => "Memory (RAM) usage",
        }
    }
}

/// Valor lido, já formatado (`"41 °C"`, `"12%"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub metric: Metric,
    pub value: String,
}

/// Leitor de métricas. Guarda a amostra anterior de `/proc/stat` para
/// calcular o uso de CPU entre duas leituras.
pub struct Sensors {
    proc: PathBuf,
    cpu_temp: Option<PathBuf>,
    gpu_temp: Option<PathBuf>,
    last_cpu: Option<CpuTimes>,
}

impl Default for Sensors {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensors {
    pub fn new() -> Self {
        Self::with_roots(Path::new("/sys/class/hwmon"), Path::new("/proc"))
    }

    /// Para testes: diretórios no lugar de `/sys/class/hwmon` e `/proc`.
    pub fn with_roots(hwmon: &Path, proc: &Path) -> Self {
        let (cpu_temp, gpu_temp) = find_temps(hwmon);
        let mut s = Self {
            proc: proc.to_owned(),
            cpu_temp,
            gpu_temp,
            last_cpu: None,
        };
        // Primeira amostra, para o uso de CPU já valer na próxima leitura.
        s.last_cpu = s.cpu_times();
        s
    }

    /// Métricas que este sistema consegue informar, na ordem de [`Metric::ALL`].
    pub fn available(&self) -> Vec<Metric> {
        Metric::ALL
            .into_iter()
            .filter(|m| match m {
                Metric::CpuTemp => self.cpu_temp.is_some(),
                Metric::GpuTemp => self.gpu_temp.is_some(),
                Metric::CpuUsage => self.proc.join("stat").exists(),
                Metric::RamUsage => self.proc.join("meminfo").exists(),
            })
            .collect()
    }

    /// Lê as métricas pedidas; as que falharem ficam de fora.
    pub fn sample(&mut self, metrics: &[Metric]) -> Vec<Reading> {
        metrics
            .iter()
            .filter_map(|&metric| {
                let value = match metric {
                    Metric::CpuTemp => read_temp(self.cpu_temp.as_deref()?)?,
                    Metric::GpuTemp => read_temp(self.gpu_temp.as_deref()?)?,
                    Metric::CpuUsage => self.cpu_usage()?,
                    Metric::RamUsage => ram_usage(&self.proc)?,
                };
                Some(Reading { metric, value })
            })
            .collect()
    }

    fn cpu_times(&self) -> Option<CpuTimes> {
        CpuTimes::parse(&std::fs::read_to_string(self.proc.join("stat")).ok()?)
    }

    fn cpu_usage(&mut self) -> Option<String> {
        let now = self.cpu_times()?;
        let prev = self.last_cpu.replace(now)?;
        let total = now.total.checked_sub(prev.total)?;
        let idle = now.idle.checked_sub(prev.idle)?;
        if total == 0 {
            return None;
        }
        let busy = 100.0 * (total - idle.min(total)) as f64 / total as f64;
        Some(format!("{busy:.0}%"))
    }
}

#[derive(Debug, Clone, Copy)]
struct CpuTimes {
    idle: u64,
    total: u64,
}

impl CpuTimes {
    /// Primeira linha de `/proc/stat`:
    /// `cpu user nice system idle iowait irq softirq steal ...`.
    fn parse(stat: &str) -> Option<Self> {
        let line = stat.lines().find(|l| l.starts_with("cpu "))?;
        let v: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .take(8)
            .map(|x| x.parse().ok())
            .collect::<Option<_>>()?;
        if v.len() < 5 {
            return None;
        }
        Some(Self {
            idle: v[3] + v[4],
            total: v.iter().sum(),
        })
    }
}

fn ram_usage(proc: &Path) -> Option<String> {
    let info = std::fs::read_to_string(proc.join("meminfo")).ok()?;
    let field = |name: &str| -> Option<u64> {
        info.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    };
    let total = field("MemTotal")?;
    let available = field("MemAvailable")?;
    if total == 0 {
        return None;
    }
    let used = 100.0 * total.saturating_sub(available) as f64 / total as f64;
    Some(format!("{used:.0}%"))
}

fn read_temp(path: &Path) -> Option<String> {
    let milli: i64 = std::fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Some(format!("{} °C", (milli as f64 / 1000.0).round()))
}

/// Procura os sensores de temperatura da CPU e da GPU entre os hwmon.
fn find_temps(hwmon: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut cpu: Option<(u8, PathBuf)> = None;
    let mut gpu: Option<(u8, PathBuf)> = None;
    let Ok(entries) = std::fs::read_dir(hwmon) else {
        return (None, None);
    };
    for dir in entries.flatten().map(|e| e.path()) {
        let Ok(name) = std::fs::read_to_string(dir.join("name")) else {
            continue;
        };
        // Preferência: menor número ganha.
        let (target, rank, labels): (_, u8, &[&str]) = match name.trim() {
            "coretemp" => (&mut cpu, 0, &["Package id 0"]),
            "k10temp" | "zenpower" => (&mut cpu, 0, &["Tdie", "Tctl"]),
            "cpu_thermal" | "soc_thermal" => (&mut cpu, 1, &[]),
            "amdgpu" | "nouveau" | "radeon" => (&mut gpu, 0, &["edge"]),
            "xe" | "i915" => (&mut gpu, 1, &[]),
            _ => continue,
        };
        if target.as_ref().is_some_and(|(r, _)| *r <= rank) {
            continue;
        }
        if let Some(input) = temp_input(&dir, labels) {
            *target = Some((rank, input));
        }
    }
    (cpu.map(|(_, p)| p), gpu.map(|(_, p)| p))
}

/// O `tempN_input` com o primeiro rótulo encontrado em `labels`, ou o
/// `temp1_input` se nenhum rótulo bater.
fn temp_input(dir: &Path, labels: &[&str]) -> Option<PathBuf> {
    for want in labels {
        for n in 1..=32 {
            let label = dir.join(format!("temp{n}_label"));
            if std::fs::read_to_string(&label).is_ok_and(|l| l.trim() == *want) {
                return Some(dir.join(format!("temp{n}_input")));
            }
        }
    }
    let first = dir.join("temp1_input");
    first.exists().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Árvore falsa com coretemp (rótulo na temp2), amdgpu e nvme.
    fn fake_system() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "telinhas-sensors-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let (hw, proc) = (root.join("hwmon"), root.join("proc"));
        write(&hw.join("hwmon0/name"), "nvme\n");
        write(&hw.join("hwmon0/temp1_input"), "33000\n");
        write(&hw.join("hwmon1/name"), "coretemp\n");
        write(&hw.join("hwmon1/temp1_label"), "Core 0\n");
        write(&hw.join("hwmon1/temp1_input"), "36000\n");
        write(&hw.join("hwmon1/temp2_label"), "Package id 0\n");
        write(&hw.join("hwmon1/temp2_input"), "41499\n");
        write(&hw.join("hwmon2/name"), "amdgpu\n");
        write(&hw.join("hwmon2/temp1_label"), "edge\n");
        write(&hw.join("hwmon2/temp1_input"), "52000\n");
        write(
            &proc.join("stat"),
            "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3 4\n",
        );
        write(
            &proc.join("meminfo"),
            "MemTotal:       16000000 kB\nMemFree: 1 kB\nMemAvailable:    4000000 kB\n",
        );
        (hw, proc)
    }

    #[test]
    fn encontra_sensores_e_formata() {
        let (hw, proc) = fake_system();
        let mut s = Sensors::with_roots(&hw, &proc);
        assert_eq!(s.available(), Metric::ALL.to_vec());
        // Segunda amostra de CPU: +100 ocupado, +300 ocioso => 25%.
        write(&proc.join("stat"), "cpu  150 0 150 1100 0 0 0 0 0 0\n");
        let r = s.sample(&[
            Metric::CpuTemp,
            Metric::GpuTemp,
            Metric::CpuUsage,
            Metric::RamUsage,
        ]);
        let values: Vec<_> = r.iter().map(|r| r.value.as_str()).collect();
        assert_eq!(values, ["41 °C", "52 °C", "25%", "75%"]);
    }

    #[test]
    fn sem_sensores_nao_oferece_temperaturas() {
        let (_, proc) = fake_system();
        let s = Sensors::with_roots(Path::new("/caminho/inexistente"), &proc);
        assert_eq!(s.available(), [Metric::CpuUsage, Metric::RamUsage]);
    }
}
