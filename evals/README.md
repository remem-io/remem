# Evaluation Harness

Evaluation suite for measuring remem's recall accuracy, consolidation quality,
and reasoning performance across different providers and models.

## Metrics

| Metric | Description |
|---|---|
| `recall@5` | Proportion of ground-truth memories in top-5 results |
| `recall@10` | Proportion of ground-truth memories in top-10 results |
| `precision` | Fraction of returned results that are genuinely relevant |
| `contradiction_detection_rate` | % of seeded contradictions correctly flagged |
| `consolidation_quality_score` | LLM-judged quality of extracted facts (1-10) |
| `latency_p50/p95` | Response time percentiles |

## Running Evaluations

You can run evaluations against a live local API server or in mock simulation mode:

### 1. Mock Simulation Mode (Default offline)
Runs a simulated performance and accuracy pass without requiring the server to be running:
```bash
python evals/benchmark.py --mock
```

### 2. Live API Server Mode
1. Start the API server in a separate terminal:
   ```bash
   cargo run -p rememhq-api -- --project eval-test
   ```
2. Run the evaluation benchmark script:
   ```bash
   python evals/benchmark.py
   ```

## Status
✅ **Evaluation harness is fully implemented (v0.2+).** The benchmark suite measures recall accuracy, semantic relevance precision, contradiction detection rate, consolidation quality, and profiles transaction latencies (p50/p95).
