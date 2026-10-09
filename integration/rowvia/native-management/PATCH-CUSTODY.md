# Rowvia Buzz patch custody

The two format-patch artifacts preserve different committed deltas. Keep their
manifests separate; updating one must not silently change the other.

| Delta | Exact commit | Exact parent | Artifact |
| --- | --- | --- | --- |
| Native management | `c3672558820f8fbaf33e8cd7497b34e415ae68e6` | `0dd1323d70ca6eaa0c93502753668381ebcd9985` | `0001-Add-owner-signed-Rowvia-management-flow.patch` and root `manifest.json` |
| External-agent enrollment | `2ea9d5391d8f90f02b9b060cd5d95b019702f3e1` | `41994cb179ae59cfd8a23122eac5e1869e4a2d6c` | `external-agent-enrollment/0001-feat-enroll-external-agents-with-owner-proof.patch` and its `manifest.json` |

The enrollment patch is exactly commit `2ea9d5391` and includes its ten
Desktop source/test files plus `EXTERNAL-AGENT-ENROLLMENT.md`. It does not
include the later Desktop-only installer commit `324cff507` or unrelated
intermediate build-tool commits. Its exact parent already contains the earlier
native management work and build tooling. These two patches alone therefore do
not constitute a complete upstream rebase recipe.

From the Buzz fork, verify each artifact's manifest keys, SHA-256, source
parent/tree, exact `git format-patch` bytes, and replayed commit ID in a
disposable checkout:

```bash
python3 integration/rowvia/native-management/verify_patch.py
python3 integration/rowvia/native-management/verify_patch.py --artifact enrollment
```

`--candidate <local tag-or-commit>` additionally checks mechanical patch
application in a separate disposable checkout. A clean apply does not prove
Rust, TypeScript, identity, approval, restart, ACP/MCP, or Desktop/Mobile
behavior. Treat a changed base, hash, tree, patch byte, or replayed commit as
a custody failure. Review every upstream seam and run those runtime gates
before accepting a release lane.

The Desktop-only installer may build from committed enrollment source
`2ea9d5391d8f90f02b9b060cd5d95b019702f3e1` using
`build-linux-desktop.sh --live --revision` with that full SHA. Its separate
installer commit is a host deployment tool, not part of the Desktop source
patch. Before deployment, the installer still requires a private reviewed pin
for that commit, the resulting live ELF SHA-256, and the exact installed
baseline hashes; no pin or build output is supplied by these custody files.
