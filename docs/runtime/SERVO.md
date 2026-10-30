# Optional Servo runtime

[Servo](https://github.com/servo/servo) is a free/open-source Rust browser engine, not Chrome, Brave, DuckDuckGo or Puppeteer. Swiftel itself remains a Rust CLI, controlling an **external** Servo process through WebDriver; Servo's distribution has its own [MPL-2.0 terms](https://github.com/servo/servo/blob/main/LICENSE). The optional Dockerfile pins Servo v0.7.0 and verifies its archive checksum. The Ubuntu base tag and apt packages are not digest/version-pinned, so Docker rebuilds are not fully reproducible; pin them in a controlled release pipeline. A successful build does not imply browser feature parity with Chromium/Firefox or future compatibility.

```bash
docker build -f infra/browser/Dockerfile -t swiftel-servo:v0.7.0 infra/browser
docker run --rm --name swiftel-servo --cap-drop=ALL --security-opt=no-new-privileges \
  --read-only --tmpfs /tmp:rw,nosuid,size=128m --shm-size=128m \
  --pids-limit=128 --memory=2g --cpus=1 -p 127.0.0.1:7000:7000 swiftel-servo:v0.7.0
# In another terminal:
swiftel browse https://example.com --acknowledge-browser-network-risk --json
```

This container command is a **runtime containment example, not an egress firewall**. Docker's default bridge can often reach the host and private networks. Enforce public-only egress using a separate network policy/firewall before browsing untrusted pages on sensitive infrastructure; do not expose WebDriver outside host loopback. `--network none` is suitable only for offline fixtures, not public browsing. Run without privileged host mounts or credentials. Browser subresources and script fetches bypass Swiftel's HTTP SSRF guard. The CLI refuses browser use without an explicit risk acknowledgement.

## Code-driven actions

```json
[{"op":"type","selector":"#query","value":"rust"},{"op":"wait","milliseconds":250},{"op":"click","selector":"#next"}]
```

```bash
swiftel browse https://example.org/ --actions ./actions.json \
  --acknowledge-browser-network-risk --json
```

Only same-origin anchor clicks, text/search input/textarea typing and waits are allowed. No buttons, form submission, arbitrary JavaScript, login, CAPTCHA solving or unrestricted navigation. Actions are capped at 20 steps, 32 KiB input, 1 KiB typed values and 2-second waits. A click target is URL-validated again before clicking, but browser scripts/subrequests can still escape the Rust networking guard. A malicious site may implement side effects on GET links; manually inspect the target and use controlled origins.

## Local AI-assisted proposal

An optional local [Ollama OpenAI-compatible endpoint](https://docs.ollama.com/api/openai-compatibility) at `127.0.0.1:11434` may propose JSON actions from a rendered snapshot. Install/run Ollama and choose a model separately; no model ships in Swiftel. The default is **preview only**:

```bash
# Save and inspect the exact proposed actions before execution:
swiftel browse https://example.org/ --prompt 'Open the next page' --model YOUR_LOCAL_MODEL \
  --acknowledge-browser-network-risk --json | jq '.ai_plan' > actions.json
swiftel browse https://example.org/ --actions actions.json \
  --acknowledge-browser-network-risk --json
# Convenience mode generates and immediately executes a NEW plan with no human preview:
swiftel browse https://example.org/ --prompt 'Open the next page' --model YOUR_LOCAL_MODEL \
  --execute-ai-plan --acknowledge-browser-network-risk --json
```

The model sees a bounded page excerpt and controls with simple HTML IDs. It cannot propose unsupported CSS selectors, arbitrary JS or cross-origin links; Swiftel validates the proposal against the snapshot and browser constraints. A new invocation generates a new plan, so use the saved `actions.json` workflow to execute a specific reviewed proposal. Execution opens a *new* browser session, and the page may change before the click. The model is untrusted, may follow prompt injection or misunderstand the page; the local planning API sends a bounded page excerpt to Ollama. This is neither autonomous agent browsing nor evidence of a semantically correct plan.

The result's `status: 0` means HTTP status unknown (not success); `sha256` hashes the rendered DOM. The browser requires a separate process and runtime libraries, even though Swiftel's implementation and control flow are Rust.
