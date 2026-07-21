# Evaluation Harness

The remem evaluation harness measures the quality, accuracy, and performance of the memory systems. It evaluates semantic retrieval, logical consistency (contradiction detection, forget operations, etc.), summarization (consolidation), and overall latency.

## Metrics Table

| Metric | Description | Unit | Default Threshold |
|---|---|---|---|
| recall@5 | Accuracy of retrieving the correct memory in the top 5 results | % | 75% |
| recall@10 | Accuracy of retrieving the correct memory in the top 10 results | % | 90% |
| semantic_precision | Precision of semantic search queries | % | 60% |
| contradiction_detection_rate | Accuracy in identifying conflicting facts | % | 50% |
| forget_correctness | Success rate of the forget (deletion/archival) operation | % | 100% |
| tag_filter_accuracy | Accuracy of filtering memories by specific tags | % | 100% |
| edge_case_pass_rate | Pass rate on pre-defined edge case tests | % | 75% |
| round_trip_accuracy | Accuracy of store and subsequent retrieve operations | % | 75% |
| consolidation_quality | Quality rating for memory consolidation and summarization | /10 | 7.0 |
| p95_store_latency | 95th percentile latency for store operations | ms | 500 |
| p95_recall_latency | 95th percentile latency for recall operations | ms | 2000 |
| p95_search_latency | 95th percentile latency for search operations | ms | 500 |

## Running Evaluations

### Mock Simulation Mode
Run tests quickly without a live API server using local mock data.
```bash
python evals/benchmark.py --mock
```

### Live API Server Mode
Run tests against a real API server instance.
```bash
# Start your server on the side, then run:
python evals/benchmark.py --base-url http://localhost:7474
```

### JSON Output
Generate a structured JSON report that can be parsed by automated tools.
```bash
python evals/benchmark.py --mock --output json --output-file results.json
```

## CLI Options

| Option | Description |
|---|---|
| `--base-url` | API server URL (default: http://localhost:7474) |
| `--api-key` | API key for authentication |
| `--mock` | Run in mock simulation mode |
| `--seed` | Random seed for deterministic mock runs (default: 42) |
| `--output` | Output format: `text` or `json` (default: text) |
| `--output-file` | Path to write the JSON results to |
| `--threshold-file` | Path to thresholds JSON (default: evals/thresholds.json) |
| `--baseline` | Path to previous JSON results for regression comparison |

## Custom Thresholds

The default thresholds are defined in `evals/thresholds.json`. This file acts as the configuration for deciding if an evaluation run passed or failed based on minimum acceptable values. You can specify a different configuration by pointing `--threshold-file` to your own custom JSON file.

## Regression Tracking

You can use the `--baseline` flag to compare current performance against a previous run to catch regressions over time.

```bash
# First run: save baseline
python evals/benchmark.py --mock --output json --output-file baseline.json

# Later run: compare against baseline  
python evals/benchmark.py --mock --output json --output-file current.json --baseline baseline.json
```

## CI Integration

To integrate the evaluation harness into GitHub Actions:

```yaml
- name: Run eval harness
  run: python evals/benchmark.py --mock --output json --output-file eval-results.json
```

## Status

**Version**: 1.0.0  
**Coverage**: Latency benchmarks, CRUD correctness, semantic precision, contradiction/forget operations, and consolidation quality mapping.
