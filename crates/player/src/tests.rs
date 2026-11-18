use super::*;
use std::io::Write;
fn wav(path: &Path, seconds: u32) {
    let length = 8000 * 2 * seconds;
    let mut f = fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + length).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0").unwrap();
    f.write_all(&8000u32.to_le_bytes()).unwrap();
    f.write_all(&16000u32.to_le_bytes()).unwrap();
    f.write_all(b"\x02\0\x10\0data").unwrap();
    f.write_all(&length.to_le_bytes()).unwrap();
    f.write_all(&vec![0u8; length as usize]).unwrap();
}
fn wait(player: &Player, predicate: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let s = player.snapshot();
        if predicate(&s) {
            return s;
        }
        assert!(Instant::now() < deadline, "Timed out: {s:?}");
        thread::sleep(Duration::from_millis(30));
    }
}
#[test]
fn scan_sorts_groups_and_numbers_and_refuses_traversal() {
    let d = tempfile::tempdir().unwrap();
    for name in ["wav", "mp3"] {
        fs::create_dir(d.path().join(name)).unwrap();
    }
    for name in ["wav/track10.wav", "wav/track2.wav", "mp3/track1.mp3"] {
        fs::write(d.path().join(name), []).unwrap();
    }
    fs::write(d.path().join("archive.zip"), []).unwrap();
    let media = media::scan("RJ1".into(), d.path()).unwrap();
    assert_eq!(
        media
            .tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["mp3/track1.mp3", "wav/track2.wav", "wav/track10.wav"]
    );
    assert!(media.has_archives);
    let alternate = d.path().join(".");
    let same = media::scan("RJ1".into(), &alternate).unwrap();
    assert_eq!(
        same.tracks
            .iter()
            .map(|track| &track.id)
            .collect::<Vec<_>>(),
        media
            .tracks
            .iter()
            .map(|track| &track.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        media::resolve(&alternate, "wav/track2.wav").unwrap(),
        d.path().join("wav/track2.wav").canonicalize().unwrap()
    );
    assert!(media::resolve(d.path(), "../outside.wav").is_err());
    assert!(media::resolve(d.path(), "/etc/passwd").is_err());
    assert!(media::resolve(d.path(), "https://example.org/file.wav").is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_are_neither_scanned_nor_played_even_after_scan() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.wav"), []).unwrap();
    symlink(external.path(), root.path().join("linked")).unwrap();
    symlink(
        external.path().join("outside.wav"),
        root.path().join("linked.wav"),
    )
    .unwrap();
    assert!(media::scan("RJ1".into(), root.path())
        .unwrap()
        .tracks
        .is_empty());
    assert!(media::resolve(root.path(), "linked/outside.wav").is_err());
    fs::write(root.path().join("changed.wav"), []).unwrap();
    assert_eq!(
        media::scan("RJ1".into(), root.path()).unwrap().tracks.len(),
        1
    );
    fs::remove_file(root.path().join("changed.wav")).unwrap();
    symlink(
        external.path().join("outside.wav"),
        root.path().join("changed.wav"),
    )
    .unwrap();
    assert!(media::resolve(root.path(), "changed.wav").is_err());
}
#[test]
fn vtt_handles_bom_identifiers_settings_overlap_and_safe_text() {
    let cues = vtt::parse("\u{feff}WEBVTT\r\n\r\nNOTE comment\r\nignored\r\n\r\nid1\r\n00:00.500 --> 00:02.000 align:start\r\n<v Alice><b>中文 &amp; 日本語</b>\r\nnext line\r\n\r\n00:01.000 --> 00:03.000\r\n&lt;not HTML&gt;\r\n").unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].start, 0.5);
    assert_eq!(cues[0].text, "中文 & 日本語\nnext line");
    assert_eq!(cues[1].text, "<not HTML>");
    assert!(vtt::parse("invalid").is_err());
    assert!(vtt::parse("WEBVTT\n\n00:00.000 --> 00:NaN\ntext").is_err());
    assert!(vtt::parse("WEBVTT\n\n00:02.000 --> 00:01.000\ntext").is_err());
}
#[test]
fn subtitle_matching_keeps_ambiguous_languages_for_manual_selection() {
    let track = Track {
        id: "wav/01.wav".into(),
        name: "01.wav".into(),
        group: "wav".into(),
        format: "WAV".into(),
        size: 0,
    };
    let sub = |id: &str| Subtitle {
        id: id.into(),
        name: id.into(),
    };
    assert_eq!(
        media::matched_subtitle(&track, &[sub("subtitles/01.vtt")]),
        Some("subtitles/01.vtt".into())
    );
    assert_eq!(
        media::matched_subtitle(&track, &[sub("subtitles/01.zh-TW.vtt")]),
        Some("subtitles/01.zh-TW.vtt".into())
    );
    assert_eq!(
        media::matched_subtitle(&track, &[sub("01.ja.vtt"), sub("01.zh-TW.vtt")]),
        None
    );
    assert_eq!(
        media::matched_subtitle(&track, &[sub("01.vtt"), sub("wav/01.vtt")]),
        Some("wav/01.vtt".into())
    );
}
#[test]
fn corrupt_history_is_not_overwritten_by_player() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("history.json");
    fs::write(&file, b"BROKEN").unwrap();
    let player = Player::start_inner(file.clone(), true, |_| {});
    player.open("RJ1".into(), root.path().into()).unwrap();
    player.shutdown();
    assert_eq!(fs::read(file).unwrap(), b"BROKEN");
}
// Opt in for machines with libmpv. No audio device or user files are used.
#[test]
#[ignore = "requires installed libmpv; uses only generated silent WAV and null audio output"]
fn native_playback_seek_subtitles_resume_and_group_eof() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("media");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("other-version")).unwrap();
    wav(&root.join("01.wav"), 12);
    wav(&root.join("02.wav"), 12);
    wav(&root.join("other-version/01.wav"), 12);
    fs::write(
        root.join("01.wav.vtt"),
        "WEBVTT\n\n00:01.000 --> 00:03.000\n字幕\n",
    )
    .unwrap();
    let history = d.path().join("history.json");
    let player = Player::start_inner(history.clone(), true, |_| {});
    let media = player.open("RJ1".into(), root.clone()).unwrap();
    assert_eq!(media.tracks.len(), 3);
    let metadata = wait(&player, |s| s.durations.len() == 3);
    assert!(metadata
        .durations
        .values()
        .all(|duration| (*duration - 12.0).abs() < 0.1));
    assert!(player
        .control(
            "stale-work".into(),
            Control::Play {
                track_id: "01.wav".into()
            }
        )
        .is_err());
    player
        .control(
            "RJ1".into(),
            Control::Play {
                track_id: "01.wav".into(),
            },
        )
        .unwrap();
    let state = wait(&player, |s| !s.loading && s.duration > 11.0);
    assert!(state.error.is_none(), "{state:?}");
    assert_eq!(state.subtitle_id, Some("01.wav.vtt".into()));
    assert_eq!(
        player.subtitle("RJ1".into(), "01.wav.vtt".into()).unwrap()[0].text,
        "字幕"
    );
    player
        .control("RJ1".into(), Control::Pause { paused: true })
        .unwrap();
    player
        .control("RJ1".into(), Control::Seek { position: 4.0 })
        .unwrap();
    wait(&player, |s| (s.position - 4.0).abs() < 0.2 && s.paused);
    player
        .control("RJ1".into(), Control::Offset { offset: 1.25 })
        .unwrap();
    player
        .control("RJ1".into(), Control::Subtitle { subtitle_id: None })
        .unwrap();
    player
        .control("RJ1".into(), Control::Volume { volume: 20.0 })
        .unwrap();
    player
        .control("RJ1".into(), Control::Speed { speed: 1.25 })
        .unwrap();
    player.shutdown();
    let player = Player::start_inner(history, true, |_| {});
    player.open("RJ1".into(), root).unwrap();
    let s = player.snapshot();
    assert_eq!(s.track_id, Some("01.wav".into()));
    assert!(s.paused);
    assert!((s.position - 4.0).abs() < 0.5, "restored: {s:?}");
    assert_eq!(s.subtitle_id, None);
    assert_eq!(s.subtitle_offset, 1.25);
    assert_eq!(s.volume, 20.0);
    assert_eq!(s.speed, 1.25);
    player
        .control("RJ1".into(), Control::Pause { paused: false })
        .unwrap();
    wait(&player, |s| !s.loading && s.duration > 0.0);
    player
        .control("RJ1".into(), Control::Seek { position: 11.8 })
        .unwrap();
    wait(&player, |s| {
        s.track_id.as_deref() == Some("02.wav") && !s.loading
    });
    player
        .control("RJ1".into(), Control::Seek { position: 11.8 })
        .unwrap();
    let s = wait(&player, |s| s.paused);
    assert_eq!(s.track_id.as_deref(), Some("02.wav"));
    player
        .control("RJ1".into(), Control::Pause { paused: false })
        .unwrap();
    let restarted = wait(&player, |s| !s.loading && !s.paused && s.position < 2.0);
    assert_eq!(restarted.track_id.as_deref(), Some("02.wav"));
    player.control("RJ1".into(), Control::Stop).unwrap();
    assert!(player.snapshot().track_id.is_none());
    assert!(player
        .control(
            "RJ1".into(),
            Control::Play {
                track_id: "../escape.wav".into()
            }
        )
        .is_err());
    player.shutdown();
}

#[test]
fn multiple_formats_in_one_folder_are_separate_versions() {
    let d = tempfile::tempdir().unwrap();
    for file in ["01.wav", "02.wav", "01.mp3", "02.mp3"] {
        fs::write(d.path().join(file), []).unwrap();
    }
    let media = media::scan("RJ1".into(), d.path()).unwrap();
    assert_eq!(
        media
            .tracks
            .iter()
            .map(|t| t.group.as_str())
            .collect::<Vec<_>>(),
        ["MP3", "MP3", "WAV", "WAV"]
    );
}

#[test]
#[ignore = "requires installed libmpv and ffmpeg; all generated audio uses null output"]
fn native_codec_probe_and_missing_file_errors() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("media");
    fs::create_dir(&root).unwrap();
    wav(&root.join("track.wav"), 3);
    for extension in ["flac", "mp3", "m4a", "ogg"] {
        let status = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(root.join("track.wav"))
            .arg(root.join(format!("track.{extension}")))
            .status()
            .expect("ffmpeg for synthetic codec fixtures");
        assert!(status.success());
    }
    let player = Player::start_inner(d.path().join("history.json"), true, |_| {});
    player.open("RJ1".into(), root.clone()).unwrap();
    wait(&player, |s| s.durations.len() == 5);
    for extension in ["wav", "flac", "mp3", "m4a", "ogg"] {
        player
            .control(
                "RJ1".into(),
                Control::Play {
                    track_id: format!("track.{extension}"),
                },
            )
            .unwrap();
        let state = wait(&player, |s| !s.loading && s.duration > 2.0);
        assert!(state.error.is_none(), "{extension}: {state:?}");
    }
    player.control("RJ1".into(), Control::Stop).unwrap();
    fs::remove_file(root.join("track.wav")).unwrap();
    assert!(player
        .control(
            "RJ1".into(),
            Control::Play {
                track_id: "track.wav".into()
            }
        )
        .is_err());
    assert!(player.snapshot().error.is_some());
    player.shutdown();
}

#[test]
#[ignore = "requires installed libmpv and an audio device; generated silent audio only"]
fn native_system_audio_output_smoke() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("media");
    fs::create_dir(&root).unwrap();
    wav(&root.join("silence.wav"), 3);
    let player = Player::start(d.path().join("history.json"), |_| {});
    player.open("RJ1".into(), root).unwrap();
    player
        .control(
            "RJ1".into(),
            Control::Play {
                track_id: "silence.wav".into(),
            },
        )
        .unwrap();
    let state = wait(&player, |s| s.position > 0.1 && !s.paused && !s.loading);
    assert!(state.error.is_none(), "{state:?}");
    player.control("RJ1".into(), Control::Stop).unwrap();
    player.shutdown();
}

fn match_subtitle(track: &str, files: &[&str]) -> Option<String> {
    let path = Path::new(track);
    let track = Track {
        id: track.into(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        group: String::new(),
        format: "WAV".into(),
        size: 0,
    };
    let files = files
        .iter()
        .map(|id| Subtitle {
            id: (*id).into(),
            name: (*id).into(),
        })
        .collect::<Vec<_>>();
    media::matched_subtitle(&track, &files)
}
#[test]
fn subtitle_matches_appended_audio_filename_before_plain_stem() {
    assert_eq!(
        match_subtitle("WAV/ABC.wav", &["subtitles/ABC.wav.vtt"]),
        Some("subtitles/ABC.wav.vtt".into())
    );
    assert_eq!(
        match_subtitle("WAV/ABC.wav", &["WAV/ABC.vtt", "subtitles/ABC.wav.vtt"]),
        Some("subtitles/ABC.wav.vtt".into())
    );
}

#[test]
fn subtitle_match_handles_case_language_suffixes_and_separate_folders() {
    for name in [
        "ABC.WAV.VTT",
        "ABC.wav.zh-TW.vtt",
        "ABC.wav_zh_CN.vtt",
        "ABC.wav（繁體中文）.vtt",
        "ABC.wav [ja].vtt",
        "ABC.wav_ja_JP.vtt",
        "ABC.wav_en_US.vtt",
        "ABC.zh-hant.vtt",
        "ABC_en.vtt",
    ] {
        let id = format!("字幕/{name}");
        assert_eq!(match_subtitle("WAV/ABC.wav", &[&id]), Some(id), "{name}");
    }
    assert_eq!(
        match_subtitle("WAV/ABC.wav", &["字幕/ABC.wav.vtt", "WAV/ABC.wav.vtt"]),
        Some("WAV/ABC.wav.vtt".into())
    );
    assert_eq!(
        match_subtitle("WAV/ABC.wav", &["字幕/ABC.wav.vtt", "字幕/ABC.wav.ja.vtt"]),
        Some("字幕/ABC.wav.vtt".into())
    );
    assert_eq!(
        match_subtitle(
            "WAV/ABC.wav",
            &["字幕/ABC.wav.ja.vtt", "字幕/ABC.wav.zh-TW.vtt"]
        ),
        None
    );
    assert_eq!(
        match_subtitle("WAV/ABC.wav", &["CN/ABC.wav.vtt", "JP/ABC.wav.vtt"]),
        None
    );
}
#[test]
fn subtitle_match_normalizes_width_unicode_punctuation_and_track_zeroes() {
    for (track, subtitle) in [
        ("＃０１　おやすみ.wav", "字幕/01-おやすみ.wav.vtt"),
        ("Track 01 - Good Night.wav", "字幕/track1_goodnight.vtt"),
        ("01_ガイド.wav", "字幕/1-ガイド.wav.vtt"),
        ("01 - Sleep (Part A).wav", "字幕/01_Sleep_Part_A.wav.vtt"),
    ] {
        assert_eq!(
            match_subtitle(track, &[subtitle]),
            Some(subtitle.into()),
            "{track}"
        );
    }
}
#[test]
fn subtitle_match_accepts_only_unique_high_similarity_typos() {
    assert_eq!(
        match_subtitle(
            "01 - Good Night Whispering.wav",
            &["subs/01-Good-Night-Whisperng.wav.vtt"]
        ),
        Some("subs/01-Good-Night-Whisperng.wav.vtt".into())
    );
    assert_eq!(
        match_subtitle(
            "01 - Good Night Whispering.wav",
            &[
                "subs/01-Good-Night-Whisperng.wav.vtt",
                "subs/01-Good-Night-Whispring.wav.vtt"
            ]
        ),
        None
    );
    // A lower-confidence fuzzy candidate cannot break an exact or normalized tie.
    assert_eq!(
        match_subtitle(
            "01 - Good Night Whispering.wav",
            &[
                "CN/01 - Good Night Whispering.vtt",
                "JP/01 - Good Night Whispering.vtt",
                "subs/01-Good-Night-Whisperng.wav.vtt"
            ]
        ),
        None
    );
}
#[test]
fn subtitle_match_rejects_different_numbers_formats_and_short_or_partial_titles() {
    for (track, subtitles) in [
        (
            "Track 01 - Good Night Whispering.wav",
            vec!["Track 02 - Good Night Whispering.wav.vtt"],
        ),
        ("01_02_Title.wav", vec!["012_Title.wav.vtt"]),
        (
            "Chapter 1.2 - Long Title.wav",
            vec!["Chapter 12 - Long Title.wav.vtt"],
        ),
        ("ABC.wav", vec!["ABC.mp3.vtt", "ABC.flac.ja.vtt"]),
        ("ABC.wav", vec!["ABD.vtt", "ABC2.vtt", "ABC_extra.vtt"]),
        (
            "01 - Good Night Whispering.wav",
            vec!["Good Night Whispering.wav.vtt"],
        ),
        (
            "01 - Good Night Whispering.wav",
            vec!["01 - Completely Different.wav.vtt"],
        ),
    ] {
        assert_eq!(
            match_subtitle(track, &subtitles),
            None,
            "{track}: {subtitles:?}"
        );
    }
}
#[test]
fn subtitle_match_is_independent_of_directory_iteration_order() {
    let files = ["JP/ABC.wav.vtt", "WAV/ABC.wav.vtt", "CN/ABC.vtt"];
    for rotation in 0..files.len() {
        let mut files = files;
        files.rotate_left(rotation);
        assert_eq!(
            match_subtitle("WAV/ABC.wav", &files),
            Some("WAV/ABC.wav.vtt".into())
        );
    }
}
#[test]
fn subtitle_bookmarks_preserve_manual_choice_and_off_but_recover_missing_files() {
    let d = tempfile::tempdir().unwrap();
    let track = "ABC.wav";
    fs::write(d.path().join(track), []).unwrap();
    fs::write(d.path().join("ABC.wav.vtt"), "WEBVTT\n").unwrap();
    fs::write(d.path().join("manual.vtt"), "WEBVTT\n").unwrap();
    let history_path = d.path().join("history.json");
    for (saved, expected) in [
        (Some("manual.vtt"), Some("manual.vtt")),
        (None, None),
        (Some("removed.vtt"), Some("ABC.wav.vtt")),
    ] {
        let history = History {
            works: BTreeMap::from([(
                "RJ1".into(),
                Bookmark {
                    track_id: track.into(),
                    subtitles: BTreeMap::from([(track.into(), saved.map(str::to_owned))]),
                    ..Bookmark::default()
                },
            )]),
            ..History::default()
        };
        history.save(&history_path).unwrap();
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        let mut worker = Worker::new(history_path.clone(), shared, true);
        worker.open("RJ1".into(), d.path().into()).unwrap();
        assert_eq!(worker.state.subtitle_id.as_deref(), expected);
    }
}
#[test]
fn bounded_edit_distance_agrees_with_full_distance_on_small_inputs() {
    let words = [
        "", "a", "ab", "abc", "acb", "xyz", "abcdef", "abqdef", "abqde", "abcdefgh",
    ];
    for a in words {
        for b in words {
            let a = a.chars().collect::<Vec<_>>();
            let b = b.chars().collect::<Vec<_>>();
            let mut matrix = vec![vec![0; b.len() + 1]; a.len() + 1];
            for (i, row) in matrix.iter_mut().enumerate() {
                row[0] = i;
            }
            for (j, value) in matrix[0].iter_mut().enumerate() {
                *value = j;
            }
            for i in 1..=a.len() {
                for j in 1..=b.len() {
                    matrix[i][j] = (matrix[i - 1][j] + 1)
                        .min(matrix[i][j - 1] + 1)
                        .min(matrix[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
                }
            }
            let distance = matrix[a.len()][b.len()];
            assert_eq!(
                media::small_edit_distance(&a, &b),
                (distance <= 2).then_some(distance),
                "{a:?}, {b:?}"
            );
        }
    }
}

#[test]
fn literal_track_stems_that_look_like_suffixes_are_preserved() {
    assert_eq!(
        match_subtitle("ABC.en.wav", &["ABC.en.vtt"]),
        Some("ABC.en.vtt".into())
    );
    assert_eq!(
        match_subtitle("ABC.wav.wav", &["ABC.wav.vtt"]),
        Some("ABC.wav.vtt".into())
    );
    assert_eq!(
        match_subtitle("ABC.mp3.wav", &["ABC.mp3.vtt"]),
        Some("ABC.mp3.vtt".into())
    );
}

fn archive_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn archive_ids_reject_paths_and_malformed_encoding() {
    for id in [
        "archive:ff/a.wav",
        "archive:0/a.wav",
        "archive:éé/a.wav",
        "archive:2f/../a.wav",
        "archive:2f//a.wav",
    ] {
        assert!(archive::location(id).is_err(), "{id}");
    }
    let id = archive::id("日本語/a%|.zip", "音声/01.wav");
    assert_eq!(
        archive::location(&id).unwrap(),
        Some(("日本語/a%|.zip".into(), "音声/01.wav"))
    );
}
#[test]
#[ignore = "requires libarchive; only generated archive contents are used"]
fn archive_scan_subtitles_encoding_and_unsafe_entries() {
    let d = tempfile::tempdir().unwrap();
    let zip = d.path().join("作品%|.zip");
    archive_zip(
        &zip,
        &[
            ("音声/01.wav", b"audio"),
            (
                "音声/01.wav.vtt",
                b"WEBVTT\n\n00:00.000 --> 00:01.000\ninside\n",
            ),
            ("../outside.wav", b"bad"),
            ("/absolute.wav", b"bad"),
            ("readme.txt", b"text"),
        ],
    );
    fs::write(
        d.path().join("01.wav.vtt"),
        "WEBVTT\n\n00:00.000 --> 00:01.000\nsidecar\n",
    )
    .unwrap();
    let media = media::scan("RJ1".into(), d.path()).unwrap();
    assert!(media.warnings.is_empty(), "{:?}", media.warnings);
    assert_eq!(media.tracks.len(), 1);
    let track = &media.tracks[0];
    assert_eq!(track.name, "01.wav");
    assert_eq!(track.size, 5);
    let inside = archive::id("作品%|.zip", "音声/01.wav.vtt");
    assert_eq!(
        media::matched_subtitle(track, &media.subtitles),
        Some(inside.clone())
    );
    assert_eq!(
        media::matched_subtitle(
            track,
            &media
                .subtitles
                .iter()
                .filter(|s| !s.id.starts_with("archive:"))
                .cloned()
                .collect::<Vec<_>>()
        ),
        Some("01.wav.vtt".into())
    );
    let other = Subtitle {
        id: archive::id("other.zip", "音声/01.wav.vtt"),
        name: "other".into(),
    };
    assert_eq!(media::matched_subtitle(track, &[other]), None);
    assert!(media::playback_source(d.path(), &track.id)
        .unwrap()
        .contains("%25%7C.zip|音声/01.wav"));
    assert_eq!(
        vtt::parse(
            std::str::from_utf8(&media::subtitle_bytes(d.path(), &inside).unwrap()).unwrap()
        )
        .unwrap()[0]
            .text,
        "inside"
    );
    assert_eq!(
        fs::read_dir(d.path()).unwrap().count(),
        2,
        "Scanning/reading must not extract files"
    );

    // ASCII ZIP headers with a CP932 filename, as used by Japanese Windows ZIPs.
    let cp932 = d.path().join("cp932.zip");
    archive_zip(&cp932, &[("1234.wav", b"audio")]);
    let bytes = fs::read(&cp932).unwrap();
    let name = encoding_rs::SHIFT_JIS.encode("音声.wav").0.into_owned();
    let bytes = bytes
        .windows(8)
        .enumerate()
        .filter_map(|(i, b)| (b == b"1234.wav").then_some(i))
        .collect::<Vec<_>>()
        .into_iter()
        .fold(bytes, |mut b, i| {
            b[i..i + 8].copy_from_slice(&name);
            b
        });
    fs::write(&cp932, bytes).unwrap();
    let entries = archive::entries(&cp932).unwrap();
    assert_eq!(entries[0].name, "音声.wav");
    assert_eq!(entries[0].stream_name, "mpv_unknown#0");
    fs::write(d.path().join("broken.zip"), b"broken").unwrap();
    let media = media::scan("RJ1".into(), d.path()).unwrap();
    assert_eq!(media.tracks.len(), 2);
    assert_eq!(media.warnings.len(), 1);
    let large = vec![b'x'; 8 * 1024 * 1024 + 1];
    archive_zip(&d.path().join("large.zip"), &[("large.vtt", &large)]);
    assert!(matches!(
        media::subtitle_bytes(d.path(), &archive::id("large.zip", "large.vtt")),
        Err(Error::InvalidSubtitle)
    ));
}
#[test]
#[ignore = "requires libmpv and libarchive; generated silent compressed WAV, null audio"]
fn native_archive_playback_seek_subtitles_and_no_extraction() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("作品%| dir");
    fs::create_dir(&root).unwrap();
    let wav_path = d.path().join("source.wav");
    wav(&wav_path, 12);
    let bytes = fs::read(&wav_path).unwrap();
    archive_zip(
        &root.join("音声.zip"),
        &[
            ("音声/01.wav", &bytes),
            (
                "音声/01.wav.vtt",
                b"WEBVTT\n\n00:01.000 --> 00:03.000\nsubtitle\n",
            ),
            ("音声/02.wav", &bytes),
        ],
    );
    let player = Player::start_inner(d.path().join("history.json"), true, |_| {});
    let media = player.open("RJ1".into(), root.clone()).unwrap();
    assert_eq!(media.tracks.len(), 2);
    player
        .control(
            "RJ1".into(),
            Control::Play {
                track_id: media.tracks[0].id.clone(),
            },
        )
        .unwrap();
    let s = wait(&player, |s| s.duration > 11.9 && !s.loading);
    assert!(s.error.is_none(), "{s:?}");
    assert_eq!(
        player
            .subtitle("RJ1".into(), s.subtitle_id.unwrap())
            .unwrap()[0]
            .text,
        "subtitle"
    );
    player
        .control("RJ1".into(), Control::Seek { position: 8.0 })
        .unwrap();
    wait(&player, |s| s.position > 8.1);
    player
        .control("RJ1".into(), Control::Pause { paused: true })
        .unwrap();
    player
        .control("RJ1".into(), Control::Seek { position: 2.0 })
        .unwrap();
    wait(&player, |s| s.position < 2.5 && s.paused);
    player
        .control("RJ1".into(), Control::Next { direction: 1 })
        .unwrap();
    wait(&player, |s| {
        s.track_id.as_ref() == Some(&media.tracks[1].id) && s.duration > 11.9
    });
    player.shutdown();
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    let saved = fs::read_to_string(d.path().join("history.json")).unwrap();
    assert!(saved.contains("archive:"));
}

#[test]
#[ignore = "requires libmpv; generated silent WAV and null audio output"]
fn native_seek_recovers_after_delayed_polling_at_high_speed() {
    let d = tempfile::tempdir().unwrap();
    wav(&d.path().join("track.wav"), 45);
    let state = Arc::new(Mutex::new(Snapshot {
        paused: true,
        volume: 70.0,
        speed: 1.0,
        ..Snapshot::default()
    }));
    let mut worker = Worker::new(d.path().join("history.json"), state, true);
    worker.open("RJ1".into(), d.path().into()).unwrap();
    worker
        .control(
            "RJ1",
            Control::Play {
                track_id: "track.wav".into(),
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while worker.state.loading {
        worker.tick();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    worker
        .control("RJ1", Control::Speed { speed: 3.0 })
        .unwrap();
    for target in [10.0, 3.0, 22.0] {
        worker
            .control("RJ1", Control::Seek { position: target })
            .unwrap();
        // Simulate a busy GUI/worker. At 3x playback, the old 0.5s tolerance
        // would miss the requested time and freeze the published clock forever.
        thread::sleep(Duration::from_millis(600));
        worker.tick();
        assert!(
            worker.seek_target.is_none(),
            "seek stayed pending: {:?}",
            worker.state
        );
        assert!(
            worker.state.position > target + 0.5 && worker.state.position < target + 4.0,
            "{:?}",
            worker.state
        );
    }
    for target in [15.0, 26.0, 5.0] {
        worker
            .control("RJ1", Control::Seek { position: target })
            .unwrap();
    }
    thread::sleep(Duration::from_millis(600));
    worker.tick();
    assert!(
        worker.state.position > 5.5 && worker.state.position < 9.0,
        "{:?}",
        worker.state
    );
    worker
        .control("RJ1", Control::Pause { paused: true })
        .unwrap();
    worker
        .control("RJ1", Control::Seek { position: 20.0 })
        .unwrap();
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(20));
        worker.tick();
    }
    assert_eq!(worker.state.position, 20.0);
    assert!(worker.state.paused);
    worker
        .control("RJ1", Control::Pause { paused: false })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while worker.seek_target.is_some() {
        thread::sleep(Duration::from_millis(20));
        worker.tick();
        assert!(Instant::now() < deadline, "{:?}", worker.state);
    }
    assert!(
        worker.state.position >= 19.8 && worker.state.position < 23.0,
        "{:?}",
        worker.state
    );
}
