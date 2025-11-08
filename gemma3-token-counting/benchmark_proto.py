#!/usr/bin/env python3
"""Benchmark protobuf-based encoding methods."""

import sentencepiece as spm
import time
from statistics import mean, stdev

# Load tokenizer
sp = spm.SentencePieceProcessor()
sp.load('gemma3_cleaned_262144_v2.spiece.model')

def time_function(func, iterations=500):
    """Time a function over multiple iterations."""
    times = []

    # Warmup
    for _ in range(10):
        func()

    # Actual timing
    for _ in range(iterations):
        start = time.perf_counter()
        result = func()
        end = time.perf_counter()
        times.append(end - start)

    return {
        'mean': mean(times),
        'stdev': stdev(times) if len(times) > 1 else 0,
        'min': min(times),
        'max': max(times)
    }

print("=" * 80)
print("PROTOBUF ENCODING METHODS BENCHMARK")
print("=" * 80)

# Test with different text sizes
test_texts = {
    "short": "Hello world!",
    "medium": " ".join(["test"] * 20),  # ~100 chars
    "long": " ".join(["tokenization"] * 100),  # ~1400 chars
}

for name, text in test_texts.items():
    print(f"\n[{name.upper()}] Text length: {len(text)} chars")
    print("-" * 80)

    methods = {
        "encode()": lambda: len(sp.encode(text)),
        "encode_as_ids()": lambda: len(sp.encode_as_ids(text)),
        "encode_as_pieces()": lambda: len(sp.encode_as_pieces(text)),
    }

    # Test protobuf methods
    try:
        proto_result = sp.encode_as_immutable_proto(text)
        print(f"encode_as_immutable_proto result type: {type(proto_result)}")
        print(f"Result: {proto_result}")

        # Check if we can get length from proto
        if hasattr(proto_result, '__len__'):
            methods["encode_as_immutable_proto()"] = lambda: len(sp.encode_as_immutable_proto(text))
        elif hasattr(proto_result, 'ids'):
            methods["encode_as_immutable_proto().ids"] = lambda: len(sp.encode_as_immutable_proto(text).ids)
        elif isinstance(proto_result, bytes):
            print("Note: Returns bytes, need to parse proto to get count")
            methods["encode_as_immutable_proto() [bytes]"] = lambda: sp.encode_as_immutable_proto(text)
    except Exception as e:
        print(f"encode_as_immutable_proto failed: {e}")

    try:
        serialized = sp.encode_as_serialized_proto(text)
        print(f"\nencode_as_serialized_proto result type: {type(serialized)}")
        print(f"Length: {len(serialized)} bytes")
        methods["encode_as_serialized_proto() [bytes]"] = lambda: sp.encode_as_serialized_proto(text)
    except Exception as e:
        print(f"encode_as_serialized_proto failed: {e}")

    print(f"\n{'Method':<45} {'Time (μs)':<15} {'Relative':<10}")
    print("-" * 80)

    baseline = None
    for method_name, func in methods.items():
        stats = time_function(func)
        if baseline is None:
            baseline = stats['mean']

        relative = stats['mean'] / baseline
        print(f"{method_name:<45} {stats['mean']*1e6:<15.2f} {relative:<10.2f}x")

# Test batch processing with proto
print("\n" + "=" * 80)
print("BATCH PROCESSING WITH PROTO")
print("=" * 80)

batch_texts = [" ".join(["test"] * 20) for _ in range(100)]

print(f"\nBatch size: {len(batch_texts)}")
print(f"{'Method':<45} {'Time (μs)':<15} {'Time/text (μs)':<15}")
print("-" * 80)

# Standard batch
def batch_standard():
    return [len(enc) for enc in sp.encode(batch_texts)]

stats = time_function(batch_standard, iterations=100)
print(f"{'sp.encode(batch)':<45} {stats['mean']*1e6:<15.2f} {stats['mean']*1e6/len(batch_texts):<15.2f}")

# Try batch proto
try:
    def batch_proto():
        return [sp.encode_as_immutable_proto(text) for text in batch_texts]

    stats = time_function(batch_proto, iterations=100)
    print(f"{'[sp.encode_as_immutable_proto(t) for t in batch]':<45} {stats['mean']*1e6:<15.2f} {stats['mean']*1e6/len(batch_texts):<15.2f}")
except Exception as e:
    print(f"Batch proto failed: {e}")

print("\n" + "=" * 80)
print("CONCLUSION")
print("=" * 80)
print("""
The protobuf methods (encode_as_immutable_proto, encode_as_serialized_proto) are
designed for serialization/deserialization, not for token counting.

They return serialized protobuf data which would need to be parsed to extract
the token count, adding significant overhead.

For token counting, stick with: sp.encode() or sp.encode_as_ids()
""")
