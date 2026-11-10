//! Обновление с GitHub Releases: при запуске смотрим последний релиз, новее нашего — в настройках появляется
//! кнопка. По ней качаем архив своей платформы, подменяем бинарь и перезапускаемся.
//! Работает, только пока релизы открыты всем: у закрытого репозитория GitHub отвечает 404, и мы просто молчим

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};

pub const REPO: &str = "qweowix/Gargantua";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(windows)]
const ASSET: &str = "Gargantua-windows-x86_64.zip";
#[cfg(not(windows))]
const ASSET: &str = "Gargantua-linux-x86_64.tar.gz";

/// "v0.10.1" → [0, 10, 1]; мусор в хвосте ("-beta") не считаем
fn nums(v: &str) -> Vec<u32> {
    v.trim_start_matches('v').split('.').map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)).collect()
}

pub fn newer(tag: &str, than: &str) -> bool {
    let (mut a, mut b) = (nums(tag), nums(than));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Последний релиз, если он новее нашего: (версия, ссылка на архив этой платформы)
pub fn check() -> Result<Option<(String, String)>> {
    let v: serde_json::Value = ureq::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("User-Agent", "Gargantua")
        .timeout(Duration::from_secs(10))
        .call()?
        .into_json()?;
    let tag = v["tag_name"].as_str().context("нет тега")?;
    if !newer(tag, VERSION) {
        return Ok(None);
    }
    let url = v["assets"].as_array().into_iter().flatten().find(|a| a["name"] == ASSET).and_then(|a| a["browser_download_url"].as_str());
    Ok(url.map(|u| (tag.trim_start_matches('v').to_string(), u.to_string())))
}

/// Скачать архив и подменить бинарь. Запущенный файл не трогаем: новый кладём рядом и меняем местами
/// (в Linux так можно прямо на ходу, в Windows старый exe переименовывается и удаляется при следующем запуске)
pub fn install(url: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let dir = exe.parent().context("нет папки программы")?;
    let tmp = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("BlackHole").join("update");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let arc = tmp.join(ASSET);
    let mut r = ureq::get(url).set("User-Agent", "Gargantua").timeout(Duration::from_secs(600)).call()?.into_reader();
    std::io::copy(&mut r, &mut std::fs::File::create(&arc)?)?;
    // tar есть и в Linux, и в Windows 10+ (там он же распаковывает zip)
    let st = crate::util::command(Path::new("tar")).arg("-xf").arg(&arc).arg("-C").arg(&tmp).status().context("нет tar")?;
    if !st.success() {
        bail!("архив не распаковался");
    }
    let name = if cfg!(windows) { "Gargantua.exe" } else { "blackhole" };
    let new = find(&tmp, name).context("в архиве нет программы")?;
    let fresh = dir.join(format!("{}.new", exe.file_name().and_then(|n| n.to_str()).unwrap_or(name)));
    std::fs::copy(&new, &fresh).with_context(|| format!("не могу писать в {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(windows)]
    {
        let old = exe.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old)?;
    }
    std::fs::rename(&fresh, &exe)?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(exe)
}

/// Хвост прошлого обновления в Windows: старый exe, который нельзя было удалить, пока он работал
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find(&p, name) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_compare_by_numbers() {
        assert!(super::newer("v0.10.0", "0.9.9"));
        assert!(super::newer("v0.2.1", "0.2.0"));
        assert!(super::newer("1.0", "0.9.9"));
        assert!(!super::newer("v0.2.0", "0.2.0"));
        assert!(!super::newer("v0.2", "0.2.0"));
        assert!(!super::newer("v0.1.9", "0.2.0"));
    }
}
