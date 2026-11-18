use crate::{archive, Error, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{
    cmp::Ordering,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub name: String,
    pub group: String,
    pub format: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subtitle {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub work_id: String,
    pub tracks: Vec<Track>,
    pub subtitles: Vec<Subtitle>,
    pub has_archives: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

// No symlinks, network URLs, or caller-supplied absolute paths are accepted.
// Recheck at the time of playback/read so a stale scan cannot grant new access.
pub fn resolve(root: &Path, id: &str) -> Result<PathBuf> {
    let relative = Path::new(id);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::InvalidPath);
    }
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(Error::InvalidPath);
    }
    let root = root.canonicalize()?;
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(Error::InvalidPath);
        }
    }
    let path = path.canonicalize()?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(Error::InvalidPath);
    }
    Ok(path)
}

pub fn scan(work_id: String, root: &Path) -> Result<Media> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(Error::InvalidPath);
    }
    let root = root.canonicalize()?;
    let mut media = Media {
        work_id,
        tracks: vec![],
        subtitles: vec![],
        has_archives: false,
        warnings: Vec::new(),
    };
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut visited = 0;
    while let Some((dir, depth)) = pending.pop() {
        if fs::symlink_metadata(&dir)?.file_type().is_symlink()
            || !dir.canonicalize()?.starts_with(&root)
        {
            continue;
        }
        if depth > 32 {
            return Err(Error::ScanLimit);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            visited += 1;
            if visited > 30_000 {
                return Err(Error::ScanLimit);
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                pending.push((path, depth + 1));
            } else if kind.is_file() {
                let relative = path.strip_prefix(&root).map_err(|_| Error::InvalidPath)?;
                let Some(parts) = relative
                    .components()
                    .map(|part| part.as_os_str().to_str())
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let id = parts.join("/");
                let extension = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let name = entry.file_name().to_string_lossy().into_owned();
                match extension.as_str() {
                    extension if is_audio_extension(extension) => {
                        media.tracks.push(Track {
                            id: id.clone(),
                            name,
                            group: parts[..parts.len() - 1].join("/"),
                            format: extension.to_uppercase(),
                            size: entry.metadata()?.len(),
                        });
                    }
                    "vtt" => media.subtitles.push(Subtitle {
                        id: id.clone(),
                        name: id,
                    }),
                    "zip" | "rar" | "7z" => {
                        media.has_archives = true;
                        // Damaged/password-protected archives must not hide other
                        // playable files in this work.
                        match archive::entries(&path) {
                            Ok(entries) => {
                                visited += entries.len();
                                if visited > 30_000 {
                                    return Err(Error::ScanLimit);
                                }
                                for member in entries {
                                    let member_path = Path::new(&member.name);
                                    let extension = member_path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_ascii_lowercase();
                                    let member_id = archive::id(&id, &member.name);
                                    if is_audio_extension(&extension) {
                                        let parent = member_path
                                            .parent()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("");
                                        media.tracks.push(Track {
                                            id: member_id,
                                            name: member_path
                                                .file_name()
                                                .unwrap()
                                                .to_string_lossy()
                                                .into_owned(),
                                            group: if parent.is_empty() {
                                                id.clone()
                                            } else {
                                                format!("{id}/{parent}")
                                            },
                                            format: extension.to_uppercase(),
                                            size: member.size,
                                        });
                                    } else if extension == "vtt" {
                                        media.subtitles.push(Subtitle {
                                            id: member_id,
                                            name: format!("{id}/{}", member.name),
                                        });
                                    }
                                }
                            }
                            Err(Error::ScanLimit) => return Err(Error::ScanLimit),
                            Err(_) => {
                                if media.warnings.is_empty() {
                                    media.warnings.push(Error::Archive.to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // Multiple formats in one directory are versions too. Keep automatic next
    // within a single format without hiding or deleting duplicate source files.
    let mut formats =
        std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
    for track in &media.tracks {
        formats
            .entry(track.group.clone())
            .or_default()
            .insert(track.format.clone());
    }
    for track in &mut media.tracks {
        if formats
            .get(&track.group)
            .is_some_and(|formats| formats.len() > 1)
        {
            track.group = if track.group.is_empty() {
                track.format.clone()
            } else {
                format!("{} · {}", track.group, track.format)
            };
        }
    }
    media.tracks.sort_by(|a, b| {
        natural_cmp(&a.group, &b.group).then_with(|| natural_cmp(&a.name, &b.name))
    });
    media.subtitles.sort_by(|a, b| natural_cmp(&a.id, &b.id));
    Ok(media)
}

// Resolve only entries listed by the current media scan, and recheck the
// archive path/member immediately before passing a local source to mpv.
pub fn playback_source(root: &Path, id: &str) -> Result<String> {
    if let Some((archive_id, member)) = archive::location(id)? {
        let path = resolve(root, &archive_id)?;
        let entry = archive::entries(&path)?
            .into_iter()
            .find(|e| e.name == member)
            .ok_or(Error::InvalidPath)?;
        archive::stream_url(&path, &entry)
    } else {
        resolve(root, id)?
            .to_str()
            .map(str::to_owned)
            .ok_or(Error::InvalidPath)
    }
}
pub fn subtitle_bytes(root: &Path, id: &str) -> Result<Vec<u8>> {
    if let Some((archive_id, member)) = archive::location(id)? {
        return archive::subtitle(&resolve(root, &archive_id)?, member);
    }
    let path = resolve(root, id)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

// Numeric comparison without integer parsing/overflow (including long track numbers).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek(), b.peek()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let xx = x.trim_start_matches('0');
                let yy = y.trim_start_matches('0');
                let order = xx
                    .len()
                    .cmp(&yy.len())
                    .then_with(|| xx.cmp(yy))
                    .then_with(|| x.len().cmp(&y.len()));
                if !order.is_eq() {
                    return order;
                }
            }
            _ => {
                let order = a.next().cmp(&b.next());
                if !order.is_eq() {
                    return order;
                }
            }
        }
    }
}

// Prefer complete audio filenames (ABC.wav.vtt), then plain stems, normalized
// names and finally unique, conservative typo matches. Never pick by scan order.
pub fn matched_subtitle(track: &Track, subtitles: &[Subtitle]) -> Option<String> {
    use unicode_normalization::UnicodeNormalization;
    let fold = |s: &str| s.nfkc().flat_map(char::to_lowercase).collect::<String>();
    let path = Path::new(&track.id);
    let full_name = fold(path.file_name()?.to_str()?);
    let stem = fold(path.file_stem()?.to_str()?);
    let format = fold(path.extension()?.to_str()?);
    if stem.trim().is_empty() {
        return None;
    }
    let normalized = normalized_name(&stem);
    let mut best_rank = None;
    let mut best = None;
    let mut ambiguous = false;
    let track_archive = archive::location(&track.id).ok().flatten().map(|(a, _)| a);
    for sub in subtitles {
        let subtitle_archive = archive::location(&sub.id).ok().flatten().map(|(a, _)| a);
        if subtitle_archive.is_some() && subtitle_archive != track_archive {
            continue;
        }
        let sub_path = Path::new(&sub.id);
        let Some(name) = sub_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let name = fold(name);
        // Preserve literal stems that happen to end in a language/format label.
        let literal_stem = name == stem;
        let (name, language) = if literal_stem || name == full_name {
            (name.as_str(), false)
        } else {
            strip_subtitle_language(&name)
        };
        let candidate_path = Path::new(name);
        let explicit_format = if literal_stem {
            None
        } else {
            candidate_path
                .extension()
                .and_then(|s| s.to_str())
                .filter(|s| is_audio_extension(s))
        };
        // An explicitly timed MP3 subtitle must not silently match the WAV copy.
        if explicit_format.is_some_and(|f| f != format) {
            continue;
        }
        let base = if explicit_format.is_some() {
            candidate_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
        } else {
            name
        };
        let stage = if base == stem {
            0
        } else {
            let candidate = normalized_name(base);
            // Preserve all numeric tokens so punctuation/typo matching cannot
            // confuse Track 01/02, split numbers or different version numbers.
            if candidate.0.is_empty() || candidate.1 != normalized.1 {
                continue;
            }
            if candidate.0 == normalized.0 {
                1
            } else {
                let a = normalized.0.chars().collect::<Vec<_>>();
                let b = candidate.0.chars().collect::<Vec<_>>();
                let length = a.len().max(b.len());
                // Short titles are too easy to confuse. Require >=90% similarity
                // and at most two character edits for longer titles.
                if !(10..=256).contains(&length) {
                    continue;
                }
                let Some(distance) = small_edit_distance(&a, &b) else {
                    continue;
                };
                if distance * 10 > length {
                    continue;
                }
                2
            }
        };
        let rank = (
            stage,
            explicit_format.is_none(),
            language,
            sub_path.parent() != path.parent(),
        );
        match best_rank {
            None => {
                best_rank = Some(rank);
                best = Some(sub);
                ambiguous = false;
            }
            Some(previous) if rank < previous => {
                best_rank = Some(rank);
                best = Some(sub);
                ambiguous = false;
            }
            Some(previous) if rank == previous => ambiguous = true,
            _ => {}
        }
    }
    if ambiguous {
        None
    } else {
        best.map(|sub| sub.id.clone())
    }
}

fn is_audio_extension(value: &str) -> bool {
    matches!(
        value,
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wma" | "aif" | "aiff" | "alac"
    )
}

// Only recognized language labels separated from the filename are stripped.
// NFKC/lowercase folding is performed by the caller before examining suffixes.
fn strip_subtitle_language(value: &str) -> (&str, bool) {
    const LANGUAGES: &[&str] = &[
        "zh-hant",
        "zh-hans",
        "zh-tw",
        "zh-cn",
        "zh_tw",
        "zh_cn",
        "zh_hant",
        "zh_hans",
        "ja-jp",
        "ja_jp",
        "en-us",
        "en_us",
        "en-gb",
        "en_gb",
        "繁體中文",
        "繁体中文",
        "簡體中文",
        "简体中文",
        "繁體",
        "繁体",
        "簡體",
        "简体",
        "日本語",
        "日文",
        "中文",
        "英文",
        "japanese",
        "english",
        "chs",
        "cht",
        "jpn",
        "eng",
        "zh",
        "ja",
        "jp",
        "en",
    ];
    let value = value.trim();
    let separator = |c: char| c.is_whitespace() || matches!(c, '.' | '_' | '-');
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        if let Some(without_close) = value.strip_suffix(close) {
            if let Some((base, language)) = without_close.rsplit_once(open) {
                if !base.trim().is_empty() && LANGUAGES.contains(&language.trim()) {
                    return (base.trim_end_matches(separator), true);
                }
            }
        }
    }
    for language in LANGUAGES {
        if let Some(base) = value.strip_suffix(language) {
            if base.chars().last().is_some_and(separator) {
                let base = base.trim_end_matches(separator);
                if !base.is_empty() {
                    return (base, true);
                }
            }
        }
    }
    (value, false)
}

fn normalized_name(value: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut digits = Vec::new();
    let mut number = String::new();
    fn flush(number: &mut String, text: &mut String, digits: &mut Vec<String>) {
        if number.is_empty() {
            return;
        }
        let token = number.trim_start_matches('0');
        let token = if token.is_empty() { "0" } else { token };
        text.push_str(token);
        digits.push(token.to_owned());
        number.clear();
    }
    for c in value.chars() {
        if c.is_ascii_digit() {
            number.push(c);
        } else {
            flush(&mut number, &mut text, &mut digits);
            if c.is_alphanumeric() {
                text.push(c);
            }
        }
    }
    flush(&mut number, &mut text, &mut digits);
    (text, digits)
}

// Banded Levenshtein with an early exit; work stays bounded for long names and
// large subtitle folders. Three or more edits are deliberately not considered.
pub(super) fn small_edit_distance(a: &[char], b: &[char]) -> Option<usize> {
    if a.is_empty() {
        return (b.len() <= 2).then_some(b.len());
    }
    if b.is_empty() {
        return (a.len() <= 2).then_some(a.len());
    }
    if a.len().abs_diff(b.len()) > 2 {
        return None;
    }
    let mut previous = (0..=b.len()).collect::<Vec<_>>();
    let mut row = vec![usize::MAX / 2; b.len() + 1];
    for i in 1..=a.len() {
        row.fill(usize::MAX / 2);
        row[0] = i;
        let start = i.saturating_sub(2).max(1);
        let end = (i + 2).min(b.len());
        for j in start..=end {
            row[j] = (previous[j] + 1)
                .min(row[j - 1] + 1)
                .min(previous[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        if row[start..=end].iter().all(|value| *value > 2) {
            return None;
        }
        std::mem::swap(&mut previous, &mut row);
    }
    (previous[b.len()] <= 2).then_some(previous[b.len()])
}
