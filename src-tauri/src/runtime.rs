//! DeepSeek Harness Desktop — self-contained bundled runtime resolution.
//!
//! Production Harness Desktop uses its own bundled Node + pinned Harness
//! runtime instead of depending on system `node`/`npm`/`npx`/`dsh`. This module
//! locates the bundled `runtime/` directory and validates it against a
//! manifest. It performs **no** runtime network bootstrap and never silently
//! falls back to arbitrary system tooling: a missing or corrupt bundled runtime
//! is reported as a clear error (fail-closed).
//!
//! Resource layout (designed multi-platform; only `darwin-arm64` is materialized
//! tonight):
//!
//! ```text
//! runtime/
//!   manifest.json
//!   NOTICE.md
//!   darwin-arm64/
//!     node/bin/node
//!     harness/            # @deepseek-ai/dsh + full dependency closure
//!   darwin-x64/           # structurally supported, not materialized
//!   windows-x64/          # structurally supported, not materialized
//! ```

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::platform::TargetIdentity;

pub const RUNTIME_DIR_NAME: &str = "runtime";
pub const MANIFEST_NAME: &str = "manifest.json";
const MANIFEST_SCHEMA_VERSION: u32 = 1;
const HARNESS_PACKAGE: &str = "@deepseek-ai/dsh";

/// Env override for tests / isolated-HOME runs (points straight at the
/// `runtime/` directory). Also used to simulate a missing/corrupt runtime.
const ENV_RUNTIME_DIR: &str = "HD_RUNTIME_DIR";

/// Error kind for bundled-runtime resolution. Integrity failures (checksum
/// mismatch / missing integrity metadata) are distinguished so the caller can
/// refuse to fall back to the system runtime after a *security* failure, while
/// still allowing the dev-only system fallback for ordinary resolution errors.
#[derive(Debug)]
pub enum ResolveError {
    Integrity(String),
    Other(String),
}

impl ResolveError {
    pub fn is_integrity(&self) -> bool {
        matches!(self, ResolveError::Integrity(_))
    }
    pub fn message(&self) -> &str {
        match self {
            ResolveError::Integrity(m) | ResolveError::Other(m) => m,
        }
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl From<String> for ResolveError {
    fn from(s: String) -> Self {
        ResolveError::Other(s)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    pub schema_version: u32,
    pub harness: HarnessSpec,
    pub node: NodeSpec,
    pub targets: BTreeMap<String, TargetSpec>,
}

#[derive(Debug, Deserialize)]
pub struct HarnessSpec {
    pub package: String,
    pub version: String,
    pub entry: String,
}

#[derive(Debug, Deserialize)]
pub struct NodeSpec {
    pub version: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetSpec {
    pub node: String,
    pub harness: String,
    #[serde(default)]
    pub node_sha256: String,
    #[serde(default)]
    pub harness_entry_sha256: String,
}

/// A validated bundled runtime for the current target.
#[derive(Debug, Clone)]
pub struct BundledRuntime {
    pub node: PathBuf,
    pub bin_js: PathBuf,
    pub source: String,
}

/// Locate the bundled `runtime/` directory. Search order:
///   1. `HD_RUNTIME_DIR` (test/isolated-HOME hook),
///   2. the Tauri resource directory — production `Contents/Resources/runtime`,
///      or `Contents/Resources/_up_/runtime` (Tauri maps `..`-prefixed bundle
///      resources under `_up_/`),
///   3. next to the current executable (dev `target/debug/runtime`),
///   4. `CARGO_MANIFEST_DIR/../runtime` (dev from source).
///
/// Returns `None` only when no manifest-bearing runtime directory exists.
pub fn locate_runtime_root(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(d) = env::var(ENV_RUNTIME_DIR) {
        if !d.is_empty() {
            candidates.push(PathBuf::from(d));
        }
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join(RUNTIME_DIR_NAME));
        // Tauri's bundler places `../`-prefixed resources under `_up_/`.
        candidates.push(dir.join("_up_").join(RUNTIME_DIR_NAME));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(RUNTIME_DIR_NAME));
        }
    }
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        let md = PathBuf::from(manifest_dir);
        if let Some(parent) = md.parent() {
            candidates.push(parent.join(RUNTIME_DIR_NAME));
        }
        candidates.push(md.join(RUNTIME_DIR_NAME));
    }

    candidates
        .into_iter()
        .find(|c| c.join(MANIFEST_NAME).is_file())
}

/// Resolve and validate the bundled runtime rooted at `runtime_root`.
pub fn resolve(runtime_root: &Path) -> Result<BundledRuntime, ResolveError> {
    let manifest_path = runtime_root.join(MANIFEST_NAME);
    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("cannot read runtime manifest {}: {e}", manifest_path.display()))?;
    let manifest: RuntimeManifest = serde_json::from_str(&text)
        .map_err(|e| format!("invalid runtime manifest {}: {e}", manifest_path.display()))?;

    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(ResolveError::Other(format!(
            "runtime manifest schema {} unsupported (expected {MANIFEST_SCHEMA_VERSION})",
            manifest.schema_version
        )));
    }
    if manifest.harness.package != HARNESS_PACKAGE {
        return Err(ResolveError::Other(format!(
            "runtime manifest harness package {} != {HARNESS_PACKAGE}",
            manifest.harness.package
        )));
    }

    let target = TargetIdentity::current();
    let spec = manifest.targets.get(target.id).ok_or_else(|| {
        format!(
            "no bundled runtime for target {} (available: {})",
            target.id,
            manifest.targets.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;

    let node = runtime_root.join(&spec.node);
    if !node.is_file() {
        return Err(ResolveError::Other(format!(
            "bundled Node not found for {} (expected {}); runtime not materialized for this target",
            target.id,
            node.display()
        )));
    }

    let harness_dir = runtime_root.join(&spec.harness);
    let bin_js = harness_dir.join(&manifest.harness.entry);
    if !bin_js.is_file() {
        return Err(ResolveError::Other(format!(
            "bundled Harness entry not found (expected {}); runtime incomplete",
            bin_js.display()
        )));
    }

    // Verify the harness package.json matches the manifest (name + version).
    // The entry is `<harness_dir>/node_modules/@deepseek-ai/dsh/lib/bin.js`, so
    // the dsh package root is two levels up.
    let pkg_path = bin_js
        .parent()
        .and_then(|lib| lib.parent())
        .map(|d| d.join("package.json"))
        .ok_or_else(|| "bundled Harness entry has no package.json ancestor".to_string())?;
    let pkg_text = fs::read_to_string(&pkg_path)
        .map_err(|e| format!("cannot read bundled Harness package.json {}: {e}", pkg_path.display()))?;
    let pkg: serde_json::Value =
        serde_json::from_str(&pkg_text).map_err(|e| format!("invalid bundled Harness package.json: {e}"))?;
    let pkg_name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let pkg_version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("");
    if pkg_name != HARNESS_PACKAGE {
        return Err(ResolveError::Other(format!(
            "bundled Harness package.json name {pkg_name:?} != {HARNESS_PACKAGE}"
        )));
    }
    if pkg_version != manifest.harness.version {
        return Err(ResolveError::Other(format!(
            "bundled Harness version {pkg_version} != manifest {}",
            manifest.harness.version
        )));
    }

    // Integrity verification is production-default and runs BEFORE any bundled
    // code executes. Missing/mismatched integrity metadata fails closed.
    verify_checksums(&node, &bin_js, &spec.node_sha256, &spec.harness_entry_sha256)?;

    // Functional node check: the bundled binary must run and report the pinned
    // version (only reached after integrity has already passed).
    let node_version = probe_node_version(&node)?;
    if node_version != manifest.node.version {
        return Err(ResolveError::Other(format!(
            "bundled Node reports version {node_version}, expected {}",
            manifest.node.version
        )));
    }

    Ok(BundledRuntime {
        node,
        bin_js,
        source: format!(
            "bundled runtime ({}; node {} from {}, harness {})",
            target.id, manifest.node.version, manifest.node.source, manifest.harness.version
        ),
    })
}

/// Run `<node> --version` and return the trimmed stdout.
pub fn probe_node_version(node: &Path) -> Result<String, String> {
    let out = Command::new(node)
        .arg("--version")
        .output()
        .map_err(|e| format!("failed to run bundled Node {}: {e}", node.display()))?;
    if !out.status.success() {
        return Err(format!(
            "bundled Node {} exited with {:?}: {}",
            node.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().trim_start_matches('v').to_string();
    if v.is_empty() {
        return Err(format!("bundled Node {} produced no version output", node.display()));
    }
    Ok(v)
}

fn verify_checksums(node: &Path, bin_js: &Path, node_sha: &str, entry_sha: &str) -> Result<(), ResolveError> {
    if node_sha.is_empty() || entry_sha.is_empty() {
        return Err(ResolveError::Integrity(
            "runtime manifest is missing integrity checksums for this target".into(),
        ));
    }
    let node_actual = sha256_hex(&fs::read(node).map_err(|e| format!("read node: {e}"))?);
    if node_actual != node_sha {
        return Err(ResolveError::Integrity(format!(
            "bundled Node integrity check failed: expected sha256 {node_sha}, got {node_actual}"
        )));
    }
    let entry_actual = sha256_hex(&fs::read(bin_js).map_err(|e| format!("read harness entry: {e}"))?);
    if entry_actual != entry_sha {
        return Err(ResolveError::Integrity(format!(
            "bundled Harness entry integrity check failed: expected sha256 {entry_sha}, got {entry_actual}"
        )));
    }
    Ok(())
}

/// Minimal SHA-256 (hex) for startup-time integrity verification. Implemented
/// without an external dependency to keep the trust boundary small.
fn sha256_hex(data: &[u8]) -> String {
    let mut h = [
        0x6a09e667u32, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, c) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|x| format!("{x:08x}")).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Relative node path for the current target, matching the real runtime
    /// layout produced by `scripts/materialize-runtime.mjs` (macOS uses
    /// `<id>/node/bin/node`; Windows uses `<id>/node/node.exe`).
    fn fixture_node_rel() -> PathBuf {
        let id = TargetIdentity::current().id;
        if cfg!(windows) {
            PathBuf::from(id).join("node").join("node.exe")
        } else {
            PathBuf::from(id).join("node").join("bin").join("node")
        }
    }

    fn write_runtime_fixture(root: &Path, node_script: &str) {
        let id = TargetIdentity::current().id;
        let node = root.join(fixture_node_rel());
        fs::create_dir_all(node.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join(format!("{id}/harness/node_modules/@deepseek-ai/dsh/lib")))
            .unwrap();
        let mut f = fs::File::create(&node).unwrap();
        f.write_all(node_script.as_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&node, fs::Permissions::from_mode(0o755));
        }
        fs::write(
            root.join(format!("{id}/harness/node_modules/@deepseek-ai/dsh/lib/bin.js")),
            "#!/usr/bin/env node\n",
        )
        .unwrap();
        fs::write(
            root.join(format!("{id}/harness/node_modules/@deepseek-ai/dsh/package.json")),
            r#"{"name":"@deepseek-ai/dsh","version":"0.1.0-rc.7"}"#,
        )
        .unwrap();
    }

    /// Write a manifest for the fixture with **correct** integrity checksums
    /// computed from the actual fixture files, so production-default integrity
    /// verification passes for the positive cases.
    fn write_manifest_with_checksums(root: &Path, harness_version: &str, node_sha: &str) {
        let target_id = TargetIdentity::current().id;
        let bin_js = root.join(format!(
            "{target_id}/harness/node_modules/@deepseek-ai/dsh/lib/bin.js"
        ));
        let entry_sha = sha256_hex(&fs::read(&bin_js).unwrap());
        let targets = serde_json::json!({
            target_id: {
                "node": fixture_node_rel().to_string_lossy(),
                "harness": format!("{target_id}/harness"),
                "nodeSha256": node_sha,
                "harnessEntrySha256": entry_sha
            }
        });
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "harness": { "package": "@deepseek-ai/dsh", "version": harness_version, "entry": "node_modules/@deepseek-ai/dsh/lib/bin.js" },
            "node": { "version": "22.22.3", "source": "https://nodejs.org/dist/v22.22.3/" },
            "targets": targets
        });
        fs::write(
            root.join(MANIFEST_NAME),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn fixture_node_sha(root: &Path) -> String {
        sha256_hex(&fs::read(root.join(fixture_node_rel())).unwrap())
    }

    #[test]
    fn sha256_known_vector() {
        // SHA-256 of the empty string.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // SHA-256 of "abc".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    // Unix-only: the fixture "node" is a `#!/bin/sh` script that must actually
    // run to pass `probe_node_version` (a runnable fake node.exe cannot be
    // created portably on Windows). Windows exercises the full resolve+run path
    // against a real `node.exe` via the CI smoke script instead.
    #[cfg(unix)]
    #[test]
    fn manifest_resolves_current_target() {
        let root = std::env::temp_dir().join(format!("hd-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_runtime_fixture(&root, "#!/bin/sh\necho v22.22.3\n");
        write_manifest_with_checksums(&root, "0.1.0-rc.7", &fixture_node_sha(&root));

        let br = resolve(&root).expect("resolve bundled runtime");
        assert!(br.node.is_file());
        assert!(br.bin_js.is_file());
        assert!(br.source.contains("22.22.3"));
        assert!(br.source.contains("0.1.0-rc.7"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_manifest_fails_closed() {
        let root = std::env::temp_dir().join(format!("hd-rt-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert!(resolve(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn wrong_harness_version_fails_closed() {
        let root = std::env::temp_dir().join(format!("hd-rt-ver-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_runtime_fixture(&root, "#!/bin/sh\necho v22.22.3\n");
        // package.json says 0.1.0-rc.7; the manifest demands a DIFFERENT version.
        write_manifest_with_checksums(&root, "9.9.9", &fixture_node_sha(&root));
        assert!(resolve(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_node_fails_closed() {
        let root = std::env::temp_dir().join(format!("hd-rt-corrupt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_runtime_fixture(&root, "#!/bin/sh\necho v9.9.9\n"); // wrong version
        write_manifest_with_checksums(&root, "0.1.0-rc.7", &fixture_node_sha(&root));
        assert!(resolve(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn integrity_mismatch_fails_closed_and_is_distinct() {
        let root = std::env::temp_dir().join(format!("hd-rt-integrity-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_runtime_fixture(&root, "#!/bin/sh\necho v22.22.3\n");
        // Write a manifest with a deliberately WRONG node checksum.
        write_manifest_with_checksums(&root, "0.1.0-rc.7", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
        let err = resolve(&root).expect_err("integrity mismatch must fail");
        assert!(err.is_integrity(), "expected integrity error, got {err}");
        assert!(err.message().contains("integrity"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_integrity_metadata_fails_closed() {
        let root = std::env::temp_dir().join(format!("hd-rt-nosha-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_runtime_fixture(&root, "#!/bin/sh\necho v22.22.3\n");
        // Manifest without checksums.
        let target_id = TargetIdentity::current().id;
        let targets = serde_json::json!({
            target_id: {
                "node": fixture_node_rel().to_string_lossy(),
                "harness": format!("{target_id}/harness")
            }
        });
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "harness": { "package": "@deepseek-ai/dsh", "version": "0.1.0-rc.7", "entry": "node_modules/@deepseek-ai/dsh/lib/bin.js" },
            "node": { "version": "22.22.3", "source": "x" },
            "targets": targets
        });
        fs::write(root.join(MANIFEST_NAME), serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        let err = resolve(&root).expect_err("missing checksums must fail closed");
        assert!(err.is_integrity(), "expected integrity error, got {err}");
        let _ = fs::remove_dir_all(&root);
    }
}
