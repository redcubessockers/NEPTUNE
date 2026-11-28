//! Rule #1, statically: the repository only ever contains small text files
//! that we wrote. See docs/asset-policy.md.
//!
//! This is an allowlist, not a blocklist: a file passes only if its name is
//! on the list, it is not too big, it is text, and it does not smuggle data
//! inside text (long hex/base64 runs).

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const MAX_FILE_SIZE: usize = 512 * 1024;
const MAX_ENCODED_RUN: usize = 512;
// The sole repository-owned graphic is text geometry, never extracted title art.
const BRAND_LOGO_PATH: &str = "docs/assets/neptune-logo.svg";
const BRAND_GIF_PATH: &str = "docs/assets/neptune-logo-animated.gif";
const BRAND_RECIPE_PATH: &str = "tools/neptune-logo-animation.cjs";
const MAX_GIF_BYTES: usize = 256 * 1024;
const BRAND_GIF_SHA256: [u8; 32] = [0x9d, 0xc5, 0x96, 0x7c, 0x39, 0x67, 0x6f, 0xad, 0x0a, 0x55, 0xec, 0x52, 0x8d, 0x50, 0x2b, 0x69, 0x0b, 0x1a, 0xd6, 0xb5, 0x70, 0xac, 0x0f, 0x60, 0xfa, 0x6f, 0x04, 0x83, 0xd8, 0x57, 0xc5, 0x35];

const ALLOWED_EXTENSIONS: &[&str] = &[
    "rs", "toml", "md", "txt", "lock", "yml", "yaml", "json", "hlsl", "hlsli", "ps1", "cmd", "sh",
];
const ALLOWED_NAMES: &[&str] = &[
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".victory-root",
    "pre-commit",
    "LICENSE",
    "LICENSE-MIT",
    "LICENSE-APACHE",
];

/// Paths explicitly exempted, with a reason. Keep this empty if at all possible.
const EXCEPTIONS: &[(&str, &str)] = &[(
    "crates/victory-formats/src/bcpack-tables.bin",
    "the BCPack format's constant tables: nothing decodes without them and they cannot be derived (owner's decision, 2026-11-23; docs/asset-policy.md)",
)];

/// Run logs: the layer's own log of a run, scrubbed of names, kept under
/// `diagnostics/` by the owner's decision (2026-11-26; docs/asset-policy.md).
/// A log is text we wrote, but long: these may be this large as text, and
/// the longest are kept gzipped ([`RUN_LOG_GZ_MAX`]).
const RUN_LOG_MAX: usize = 32 * 1024 * 1024;
/// A gzipped run log (`diagnostics/*.txt.gz`) may be this large.
const RUN_LOG_GZ_MAX: usize = 16 * 1024 * 1024;

/// Whether `path` is a run log directly under `diagnostics/`: `Some(true)`
/// for a gzipped one (`.txt.gz`), `Some(false)` for a text one (`.txt`).
fn run_log(path: &str) -> Option<bool> {
    let name = path.strip_prefix("diagnostics/")?;
    if name.contains('/') {
        return None;
    }
    if name.ends_with(".txt.gz") {
        Some(true)
    } else if name.ends_with(".txt") {
        Some(false)
    } else {
        None
    }
}

/// Signatures of formats that must never be committed.
const FORBIDDEN_MAGIC: &[(&[u8], &str)] = &[
    (b"MZ", "PE executable"),
    (b"DXBC", "shader container"),
    (b"DDS ", "DDS texture"),
    (b"RIFF", "RIFF (audio/video)"),
    (b"BC\xC0\xDE", "LLVM bitcode"),
    (b"Microsoft C/C++ MSF", "PDB"),
    (b"!<arch>", "library archive"),
    (b"PK\x03\x04", "zip"),
    (b"7z\xBC\xAF", "7-zip"),
    (b"MSCF", "cabinet"),
    (b"\xD0\xCF\x11\xE0", "MSI/OLE"),
];

pub fn run(root: &Path, staged: bool) -> Result<()> {
    let files = list_files(root, staged)?;
    let mut violations = Vec::new();
    for path in &files {
        if EXCEPTIONS.iter().any(|(p, _)| p == path) {
            continue;
        }
        let content = if staged {
            let out = Command::new("git")
                .args(["show", &format!(":{path}")])
                .current_dir(root)
                .output()?;
            if !out.status.success() {
                continue; // deleted in the index
            }
            out.stdout
        } else {
            match std::fs::read(root.join(path)) {
                Ok(c) => c,
                Err(_) => continue,
            }
        };
        for problem in check(path, &content) {
            violations.push(format!("{path}: {problem}"));
        }
    }
    if violations.is_empty() {
        println!("asset policy: {} files ok", files.len());
        return Ok(());
    }
    for v in &violations {
        eprintln!("POLICY: {v}");
    }
    bail!(
        "{} asset-policy violation(s). Nothing from the game may be committed; \
         see docs/asset-policy.md",
        violations.len()
    )
}

fn list_files(root: &Path, staged: bool) -> Result<Vec<String>> {
    let git = |args: &[&str]| -> Result<Option<Vec<String>>> {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .context("running git")?;
        if !out.status.success() {
            return Ok(None);
        }
        Ok(Some(
            out.stdout
                .split(|&b| b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).replace('\\', "/"))
                .collect(),
        ))
    };
    if staged {
        return git(&[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ])?
        .context("--staged needs a git repository");
    }
    if let Some(mut tracked) = git(&[
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ])? {
        tracked.sort();
        tracked.dedup();
        return Ok(tracked);
    }
    // Not a git repository: walk everything except build output.
    Ok(walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some("target" | ".git")))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect())
}

/// All problems with one file. Pure, for testing.
pub fn check(path: &str, content: &[u8]) -> Vec<String> {
    let mut problems = Vec::new();
    // This single known binary is our original README artwork, not title data.
    // Every other GIF/PNG and every changed byte remains rejected.
    if path == BRAND_GIF_PATH {
        if content.len() > MAX_GIF_BYTES {
            problems.push("original GIF exceeds 256 KiB".to_owned());
        }
        if content.len() < 13
            || &content[..6] != b"GIF89a"
            || u16::from_le_bytes([content[6], content[7]]) != 1280
            || u16::from_le_bytes([content[8], content[9]]) != 300
        {
            problems.push("not the 1280x300 original GIF89a".to_owned());
        }
        if sha256(content) != BRAND_GIF_SHA256 {
            problems.push("animated identity SHA-256 mismatch".to_owned());
        }
        return problems;
    }
    // A run log has rules of its own: a larger size, and for a gzipped one
    // the gzip signature instead of the text rule. (No rule on hex runs: a
    // log quotes hashes and addresses.)
    if let Some(gzipped) = run_log(path) {
        let limit = if gzipped { RUN_LOG_GZ_MAX } else { RUN_LOG_MAX };
        if content.len() > limit {
            problems.push(format!("{} bytes exceeds the {limit}-byte limit for a run log", content.len()));
        }
        if gzipped {
            if !content.starts_with(&[0x1f, 0x8b]) {
                problems.push("a .txt.gz run log that is not a gzip stream".to_owned());
            }
        } else {
            for (magic, what) in FORBIDDEN_MAGIC {
                if content.starts_with(magic) {
                    problems.push(format!("looks like a {what}"));
                }
            }
            if content.contains(&0) {
                problems.push("binary content (NUL bytes)".to_owned());
            }
        }
        return problems;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let allowed = path == BRAND_LOGO_PATH
        || path == BRAND_RECIPE_PATH
        || ALLOWED_NAMES.contains(&name)
        || ext
            .as_deref()
            .is_some_and(|e| ALLOWED_EXTENSIONS.contains(&e));
    if !allowed {
        problems.push("file type not on the allowlist".to_owned());
    }
    if path == BRAND_LOGO_PATH {
        let markup = String::from_utf8_lossy(content).to_ascii_lowercase();
        let forbidden = [
            "<script", "<image", "<foreignobject", "<iframe", "<animate", "<set",
            "<style", "href=", "url(", "data:", "<!doctype", "<?", "onload=",
            "onbegin=", "onerror=", "onclick=", "onmouseover=", "onfocus=",
            "onpointer", "onkey", "onmouse", "ontouch", "onanimation",
            "javascript:", "@import", "style=", "src=",
        ];
        if !markup.starts_with("<svg ")
            || !markup.trim_end().ends_with("</svg>")
            || forbidden.iter().any(|token| markup.contains(token))
        {
            problems.push("branding SVG must contain only self-contained static geometry".to_owned());
        }
    }
    if content.len() > MAX_FILE_SIZE {
        problems.push(format!(
            "{} bytes exceeds the {MAX_FILE_SIZE}-byte limit",
            content.len()
        ));
    }
    for (magic, what) in FORBIDDEN_MAGIC {
        if content.starts_with(magic) {
            problems.push(format!("looks like a {what}"));
        }
    }
    if content.contains(&0) {
        problems.push("binary content (NUL bytes)".to_owned());
    } else if let Some(run) = longest_encoded_run(content)
        && run > MAX_ENCODED_RUN
    {
        problems.push(format!("{run}-character hex/base64 run (embedded data?)"));
    }
    problems
}

/// Longest run of characters that could be hex or base64 payload.
fn longest_encoded_run(content: &[u8]) -> Option<usize> {
    let is_payload = |b: &u8| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=');
    content.split(|b| !is_payload(b)).map(<[u8]>::len).max()
}


/// SHA-256 only for a content-pinned repository asset. No runtime cryptographic API.
fn sha256(bytes: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
        0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(word.try_into().expect("SHA-256 word has four bytes"));
        }
        for i in 16..64 {
            let a = w[i - 15];
            let b = w[i - 2];
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (state_word, result) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state_word = state_word.wrapping_add(result);
        }
    }
    let mut result = [0u8; 32];
    for (chunk, word) in result.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::check;

    /// Enforces the policy wherever tests run, even in clones without the hook.
    #[test]
    fn repository_passes_policy() {
        super::run(&crate::workspace_root(), false).unwrap();
    }

    /// Run logs under `diagnostics/` may be long, and gzipped; nothing else
    /// there, nothing deeper, and nothing that is not what its name says.
    #[test]
    fn run_logs_have_their_own_rules() {
        let long = vec![b'a'; 4 * 1024 * 1024];
        assert!(check("diagnostics/run-1.txt", &long).is_empty());
        assert!(check("diagnostics/run-1.txt.gz", &[0x1f, 0x8b, 8, 0, 0, 0]).is_empty());
        // Not a gzip stream; binary under a text name; too long.
        assert!(!check("diagnostics/run-1.txt.gz", b"MZ\x90\x00").is_empty());
        assert!(!check("diagnostics/run-1.txt", b"abc\0def").is_empty());
        assert!(!check("diagnostics/run-1.txt", &vec![b'a'; super::RUN_LOG_MAX + 1]).is_empty());
        assert!(!check("diagnostics/run-1.txt.gz", &[&[0x1f, 0x8b][..], &vec![0u8; super::RUN_LOG_GZ_MAX][..]].concat()).is_empty());
        // The ordinary rules everywhere else, and for other names there.
        assert!(!check("docs/run-1.txt", &long).is_empty());
        assert!(!check("diagnostics/deeper/run-1.txt", &long).is_empty());
        assert!(!check("diagnostics/shot.png", b"\x89PNG").is_empty());
    }

    #[test]
    fn accepts_source() {
        assert!(check("crates/x/src/lib.rs", b"fn main() {}\n").is_empty());
        assert!(check(".gitignore", b"/target/\n").is_empty());
        assert!(check(".githooks/pre-commit", b"#!/bin/sh\n").is_empty());
    }

    #[test]
    fn rejects_game_like_files() {
        assert!(!check("assets/tex.dds", b"DDS \x7c\0\0\0").is_empty());
        assert!(
            !check("notes.txt", b"MZ\x90\0").is_empty(),
            "magic wins over extension"
        );
        assert!(
            !check("data.bin", b"hello").is_empty(),
            "extension not allowed"
        );
        assert!(
            !check("a.rs", &vec![b'a'; 600 * 1024]).is_empty(),
            "too big"
        );
    }

    #[test]
    fn owned_neptune_svg_is_scoped_and_data_free() {
        let logo = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h1v1z\"/></svg>\n";
        assert!(check("docs/assets/neptune-logo.svg", logo).is_empty());
        assert!(!check("docs/assets/unrelated.svg", logo).is_empty());
        assert!(!check("assets/another.svg", logo).is_empty());
        assert!(!check("docs/assets/neptune-logo.png", b"not a png").is_empty());
        assert!(!check("docs/assets/neptune-logo.svg", b"wrong").is_empty());
        assert!(!check("docs/assets/neptune-logo.svg", b"<svg>\0</svg>\n").is_empty());
        assert!(!check(
            "docs/assets/neptune-logo.svg",
            b"<svg><script>alert(1)</script></svg>\n"
        ).is_empty());
        assert!(!check(
            "docs/assets/neptune-logo.svg",
            b"<svg><image href=\"https://example.com/a\"/></svg>\n"
        ).is_empty());
        assert!(!check(
            "docs/assets/neptune-logo.svg",
            b"<svg width=\"12\"><path style=\"fill:red\"/></svg>\n"
        ).is_empty());
        assert!(!check(
            "docs/assets/neptune-logo.svg",
            b"<svg width=\"12\"><path onerror=\"alert(1)\"/></svg>\n"
        ).is_empty());
        assert!(!check(
            "docs/assets/neptune-logo.svg",
            b"<svg width=\"12\"><path src=\"https://example.com/p\"/></svg>\n"
        ).is_empty());
        let hidden_data = format!("<svg>{}</svg>\n", "A1".repeat(400));
        assert!(!check("docs/assets/neptune-logo.svg", hidden_data.as_bytes()).is_empty());
    }

    #[test]
    fn sha256_matches_standard_vectors() {
        const EMPTY: [u8; 32] = [0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55];
        const ABC: [u8; 32] = [0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad];
        assert_eq!(super::sha256(b""), EMPTY);
        assert_eq!(super::sha256(b"abc"), ABC);
    }

    #[test]
    fn original_gif_is_exact_and_scope_is_narrow() {
        const IMAGE: &[u8] = include_bytes!("../../docs/assets/neptune-logo-animated.gif");
        assert!(check(super::BRAND_GIF_PATH, IMAGE).is_empty());
        assert!(!check("docs/assets/copy.gif", IMAGE).is_empty());
        assert!(!check("docs/assets/neptune-logo-static.png", IMAGE).is_empty());
        assert!(!check(super::BRAND_GIF_PATH, b"GIF89a").is_empty());
        assert!(!check(super::BRAND_GIF_PATH, &vec![0u8; super::MAX_GIF_BYTES + 1]).is_empty());
        let mut corrupted = IMAGE.to_vec();
        corrupted[70] ^= 1;
        assert!(!check(super::BRAND_GIF_PATH, &corrupted).is_empty());
        assert!(check(super::BRAND_RECIPE_PATH, b"// source only\n").is_empty());
        assert!(!check("tools/another.cjs", b"// no\n").is_empty());
        assert!(!check(super::BRAND_RECIPE_PATH, b"MZ\0").is_empty());
    }

    #[test]
    fn rejects_data_smuggled_into_text() {
        let blob = format!("const DATA: &str = \"{}\";", "A1".repeat(400));
        assert!(!check("src/blob.rs", blob.as_bytes()).is_empty());
    }
}
