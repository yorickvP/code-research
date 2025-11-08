# Gemma3 Token Counting Investigation

## Objective
Benchmark the fastest methods for counting tokens using Gemma3 SentencePiece tokenizer in Python.

## Setup
- Tokenizer URL: https://github.com/google/gemma_pytorch/raw/refs/heads/main/tokenizer/gemma3_cleaned_262144_v2.spiece.model
- Language: Python with sentencepiece library

## Investigation Log

### Initial Setup
- Created project folder: gemma3-token-counting
- Downloaded tokenizer model (4.5MB, 262,144 vocab size)

### API Exploration Findings
1. **Batch encoding is supported!** - `sp.encode(list_of_texts)` returns `[[ids], [ids], ...]`
2. Available encoding methods:
   - `encode()` / `encode_as_ids()` - Returns list of token IDs (identical performance expected)
   - `encode_as_pieces()` - Returns list of token strings
   - `encode_as_immutable_proto()` / `encode_as_serialized_proto()` - Protobuf formats
3. No direct "count tokens" method - must use `len(encode(...))`
4. Vocab size: 262,144 tokens

### Optimization Hypotheses to Test
1. Batch encoding vs individual encoding
2. `encode()` vs `encode_as_ids()` performance
3. Avoiding string conversion with `encode_as_pieces()`
4. Different batch sizes
5. Text length impact on performance

### Benchmark Results

#### Key Findings:
1. **Batching is CRITICAL**:
   - Batch size 500: **4.04x speedup** vs individual
   - Batch size 1000: **~30x faster** per text (5.67 μs vs 54.46 μs)
   - Throughput scales dramatically: 18K texts/sec (batch=1) → 176K texts/sec (batch=1000)

2. **Method performance**:
   - `encode()`: 17.04 μs (baseline)
   - `encode_as_ids()`: 17.41 μs (identical, +2%)
   - `encode_as_pieces()`: 20.02 μs (slower due to string conversion, +17%)
   - **Recommendation**: Use `encode()` or `encode_as_ids()`

3. **Count vs Full List**: NO DIFFERENCE
   - `len(sp.encode(text))`: 352.78 μs
   - `sp.encode(text)`: 351.97 μs
   - Getting the full token list is free - no need to optimize here

4. **Text length scaling**:
   - ~1.3-3.2 μs per token
   - Roughly linear with text length
   - Small texts (10 chars): 2.96 μs
   - Large texts (10K chars): 4101 μs

5. **Optimal batch size**:
   - For maximum throughput: 500-1000 texts
   - For latency/throughput balance: 100-200 texts
   - Even batch size 50 gives significant gains

### Implementation Recommendation:
```python
# FAST: Batch processing
token_counts = [len(enc) for enc in sp.encode(texts)]

# SLOW: Individual processing
token_counts = [len(sp.encode(text)) for text in texts]
```

For 500 texts, batch processing is **4x faster**!

### Additional Findings: Protobuf Methods

Tested `encode_as_immutable_proto()` and `encode_as_serialized_proto()`:

1. **encode_as_immutable_proto()** returns rich metadata:
   - Each token includes: piece, id, surface form, begin/end positions
   - Can count tokens via `len(proto.pieces)`
   - ~15% slower than `encode()` for counting
   - **Use case**: When you need token positions or surface forms

2. **encode_as_serialized_proto()** returns raw bytes:
   - Protobuf serialization format
   - Similar performance to other methods
   - Needs parsing to extract count (not recommended for counting)

3. **Batch size matters for native batching**:
   - At batch size 10: List comp is **39x faster** than native batch (4.84 vs 190.36 μs/text)
   - At batch size 100: List comp is **3.3x faster** (10.94 vs 36.49 μs/text)
   - At batch size 500: Native batch is **1.3x faster** (8.62 vs 11.23 μs/text)
   - **Crossover point**: ~200-300 texts

**Updated recommendation**:
- For batches < 200: Use list comprehension `[len(sp.encode(t)) for t in texts]`
- For batches ≥ 200: Use native batching `[len(enc) for enc in sp.encode(texts)]`
- For metadata (positions): Use `encode_as_immutable_proto()`
