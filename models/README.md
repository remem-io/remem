# Model Management

This directory documents the models `remem` knows how to download and
manage. The registry itself lives in code
(`rememhq-core/src/models/mod.rs::KNOWN_MODELS`), not as manifest files —
see "Adding a model" below if you're extending it.

## Models

### Required for local embeddings
- **nomic-embed-text-v1.5** (~275MB ONNX + vocab) — embedding model, consumed
  directly by remem's C++ (`libremem`) embedder over FFI.
  - Download: `remem models pull nomic-embed`
  - Enables: `REMEM_PROVIDER=local` embeddings (`REMEM_LOCAL_MODEL_PATH` /
    `REMEM_LOCAL_VOCAB_PATH`)

### Optional (local reasoning only)
- **phi-3-mini-4k-instruct** (~2.4GB GGUF, Q4_K_M) — local reasoning model.
  remem downloads the weights and can launch a local inference server for
  you; either way you need a llama.cpp-compatible runtime (`llama-server`,
  Ollama, LM Studio, ...) to actually serve completions from it.
  - Download: `remem models pull phi-3-mini`
  - Serve (one command): `remem models serve phi-3-mini`
    — starts `llama-server`, waits for it to become healthy, and prints
    the `REMEM_PROVIDER` / `LLAMA_API_BASE` values to export. Requires
    `llama-server` on `PATH` (from [llama.cpp](https://github.com/ggml-org/llama.cpp));
    point `REMEM_LLAMA_SERVER_BIN` at it otherwise.
  - Serve (manual): `llama-server -m ~/.remem/models/phi-3-mini-4k-instruct-q4.gguf`
  - Enables: `REMEM_PROVIDER=local` reasoning (`LLAMA_API_BASE` /
    `OLLAMA_API_BASE`) — see [`docs/PROVIDERS.md`](../docs/PROVIDERS.md)

## Checking install status

```bash
remem models list
```

or over the REST API:

```bash
curl -H "Authorization: Bearer $REMEM_API_KEY" http://localhost:8787/v1/models
```

`POST /v1/models/pull` (`{"model": "phi-3-mini"}`) downloads a model the same
way the CLI does, except the download runs in the background and the
request returns immediately (`202 Accepted`) — a multi-gigabyte download
would otherwise exceed the API's request timeout. Poll `GET /v1/models`
to see when a model's `status` becomes `"installed"`.

`remem models serve` is deliberately **CLI-only**, with no REST
equivalent: it spawns and owns an OS process on whatever machine runs the
command, which is a meaningfully different (and riskier) thing for a
remote HTTP endpoint to trigger than downloading a file — it doesn't fit
`rememhq-api`'s bearer-token-only security model. Run it locally, then
point a `rememhq-api` instance (local or remote) at it via
`LLAMA_API_BASE`.

## Checksum verification

`ModelSpec` can carry an expected SHA-256 for each artifact
(`primary_sha256` / `secondary_sha256`). When one's set, `remem models
pull` hashes the download and verifies it before moving the file into
place — a mismatch deletes the download and fails with an error rather
than silently leaving a corrupted or tampered-with file where `remem
models list`/`GET /v1/models` would otherwise just report it as
installed (they only check that the file exists, not its contents).
`phi-3-mini` has a confirmed hash today; `nomic-embed` deliberately
doesn't yet — see the comment on its `ModelSpec` entry for why (short
version: its upstream file has had several differently-sized revisions,
and a wrong hardcoded hash would hard-fail every legitimate download of
the model required for local embeddings, which is worse than not
verifying it at all).

When adding a model with a known hash, get it from the artifact's actual
Hugging Face blob page (shows the file's Git LFS `SHA256:` directly) —
not a third-party mirror/re-upload, which can differ even for the "same"
model.

## Adding a model

Add a `ModelSpec` entry to `KNOWN_MODELS` in
`rememhq-core/src/models/mod.rs`:

- `kind: ModelKind::Embedding` for an ONNX model that needs a paired
  vocab/tokenizer file (`secondary_url` / `secondary_filename`).
- `kind: ModelKind::LocalLlm` for a single-file GGUF model
  (`secondary_url`/`secondary_filename: None`).

Both the CLI (`remem models pull|list`) and the REST API
(`GET /v1/models`, `POST /v1/models/pull`) read from the same registry, so
a new entry is immediately available in both.

## Status
Local embeddings (ONNX, via `libremem`) and local reasoning (GGUF, served
by `llama-server` — either launched with `remem models serve` or run
manually) are both supported today — see
[`docs/PROVIDERS.md`](../docs/PROVIDERS.md). What's not yet implemented:
GPU-accelerated local inference (ONNX Runtime + CUDA/TensorRT/MPS), a
device manager/scheduler across multiple concurrently-served models, and
VLM/image-embedding support. Model provenance verification exists (see
above) but isn't complete — `nomic-embed`'s hash is still unconfirmed.
