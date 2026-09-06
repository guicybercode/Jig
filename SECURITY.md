# Security policy

Jig is a local-first desktop application for Linux and macOS. This policy
explains how to report a suspected vulnerability and the security boundaries
to consider before using it.

## Supported versions

Jig is beta software. Security maintenance targets the newest published
release, including a release marked as a pre-release, on a best-effort basis.
Older releases do not have a separate security-backport commitment. Check
the [release notes](https://github.com/guicybercode/Jig/releases) for the
current version, known limitations, and any available mitigations.

Reports affecting `main` are also welcome, but development builds are not
stable releases. There is no guaranteed response time, fix deadline, or
security support SLA.

## Report a vulnerability privately

Use [Report a vulnerability](https://github.com/guicybercode/Jig/security/advisories/new)
while signed in to GitHub. This opens the repository's private reporting
channel; do not open a public issue or pull request with vulnerability details.
GitHub describes this workflow in its
[private reporting guide](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/report-privately).

If the private form is unavailable, open an issue asking only for a private
security contact. Do not include the affected component, exploit, logs, or
other sensitive details in that public request.

Include, when available:

- The Jig version or commit, operating system and architecture, and whether
  you used a published package or a source build.
- A description of the impact and the trust boundary crossed.
- Minimal reproduction steps using a disposable repository and synthetic
  data, plus the expected and observed behavior.
- Sanitized diagnostics or screenshots, relevant dependency versions, and
  any mitigation you have verified.

Never attach API tokens, passwords, private keys, authentication cookies,
full environment dumps, private repository contents, or unreviewed terminal
transcripts. Inspect diagnostic exports and screenshots before sharing them:
automatic redaction cannot recognize every secret. If a credential has
already been exposed, revoke or rotate it with its provider; do not send the
credential as proof.

Maintainers may ask for additional reproduction details in the private
report. Please coordinate public disclosure there so affected versions,
mitigations, and any fix can be communicated together. Published security
notices are available in
[repository advisories](https://github.com/guicybercode/Jig/security/advisories).

## Security boundaries

- **Agent execution is not sandboxed by Jig.** A selected CLI runs with your
  local account's filesystem and network access. Separate PTYs and Git
  worktrees are coordination boundaries, not security isolation. Only start
  executables and projects you trust; any vendor-specific sandbox or
  approval mode must be configured in that CLI.
- **Jig is not a vendor proxy.** Agent CLIs manage their own authentication
  and network traffic. Report vulnerabilities specific to an agent or its
  remote service to that vendor; report unsafe Jig integration behavior here.
- **The daemon is a local control plane.** It owns sessions and communicates
  through a per-user Unix socket, not an authenticated remote API. Private
  directories use mode `0700` and the socket uses `0600`; Linux additionally
  checks the peer UID. macOS relies on filesystem ownership and permissions.
  Do not expose or forward the socket to other users or the network. Jig
  does not defend against an already-compromised process under the same
  operating-system account.
- **Remote browser pages are untrusted.** They must not gain application or
  daemon IPC access. Native browser permissions and isolation have
  platform-specific limitations; see [known issues](docs/KNOWN_ISSUES.md).
  Automated tests are not a substitute for the packaged browser acceptance
  checks in the [release checklist](docs/RELEASE_CHECKLIST.md).
- **Recovery must not act on stale process IDs.** Closing the desktop window
  does not stop daemon-owned sessions. After a daemon crash, affected session
  metadata becomes `unknown`; stored PIDs are not used to reattach or signal
  a process. Follow the [recovery guide](docs/backup-and-recovery.md) and
  verify process identity before taking manual action.

Unexpected command execution, cross-user socket access, path-validation
bypasses, unsafe worktree deletion, secret leakage, and remote-page access
to native application privileges are examples of reports in scope. Test
only on systems, repositories, and data you own or have permission to test.

## Release integrity

Current builds are unsigned, and macOS notarization is not configured.
Download packages only from this repository's releases or packaging workflow
and follow the [installation and verification guide](docs/install.md).
Compare downloaded files against the accompanying `SHA256SUMS`; checksums
detect corruption or mismatches but do not replace code signing or establish
publisher identity. Do not disable system-wide security protections to run
Jig.

For implementation details and residual risks, read the
[threat model](docs/THREAT_MODEL.md). This policy is not a certification or a
claim that every release has completed manual security acceptance.
