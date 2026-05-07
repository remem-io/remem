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

```bash
cd evals
remem eval run --provider anthropic --model claude-sonnet-4-5
remem eval run --provider openai --model gpt-4o
remem eval compare results/
```

## Status
🚧 **Evaluation harness is planned for v0.2.** The benchmark tasks will be
based on LongMemEval with rememhq-specific additions for consolidation
and contradiction detection quality.
