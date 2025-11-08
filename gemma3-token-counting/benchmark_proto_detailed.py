#!/usr/bin/env python3
"""Detailed benchmark: Can we count tokens from immutable proto efficiently?"""

import sentencepiece as spm
import time
from statistics import mean

# Load tokenizer
sp = spm.SentencePieceProcessor()
sp.load('gemma3_cleaned_262144_v2.spiece.model')

def time_function(func, iterations=200):
    """Time a function."""
    times = []
    for _ in range(10):
        func()  # warmup
    for _ in range(iterations):
        start = time.perf_counter()
        func()
        end = time.perf_counter()
        times.append(end - start)
    return mean(times)

print("=" * 80)
print("IMMUTABLE PROTO DETAILED ANALYSIS")
print("=" * 80)

# Test what the immutable proto object has
text = "Hello world test"
proto = sp.encode_as_immutable_proto(text)

print(f"\nImmutableSentencePieceText attributes:")
print(f"  - dir: {[x for x in dir(proto) if not x.startswith('_')]}")
print(f"  - pieces: {proto.pieces}")
print(f"  - len(pieces): {len(proto.pieces)}")

# Test if we can count from proto
print(f"\n{'Method':<50} {'Time (μs)':<15}")
print("-" * 80)

methods = {
    "len(sp.encode(text))": lambda: len(sp.encode(text)),
    "len(sp.encode_as_immutable_proto(text).pieces)": lambda: len(sp.encode_as_immutable_proto(text).pieces),
}

for name, func in methods.items():
    t = time_function(func, iterations=500)
    print(f"{name:<50} {t*1e6:<15.2f}")

# Now test with batching strategies
print("\n" + "=" * 80)
print("BATCH PROCESSING STRATEGIES")
print("=" * 80)

batch_sizes = [10, 50, 100, 500]
test_texts = [" ".join(["test"] * (i % 20 + 1)) for i in range(500)]

for batch_size in batch_sizes:
    texts = test_texts[:batch_size]

    print(f"\nBatch size: {batch_size}")
    print(f"{'Method':<50} {'Total (μs)':<15} {'Per text (μs)':<15}")
    print("-" * 80)

    strategies = {
        # Strategy 1: Standard batch encoding
        "sp.encode(texts) [batch native]": lambda: [len(enc) for enc in sp.encode(texts)],

        # Strategy 2: List comp with individual encode
        "[len(sp.encode(t)) for t in texts]": lambda: [len(sp.encode(t)) for t in texts],

        # Strategy 3: List comp with immutable proto
        "[len(sp.encode_as_immutable_proto(t).pieces) for t]": lambda: [len(sp.encode_as_immutable_proto(t).pieces) for t in texts],

        # Strategy 4: Just get the proto objects (if we need metadata)
        "[sp.encode_as_immutable_proto(t) for t]": lambda: [sp.encode_as_immutable_proto(t) for t in texts],
    }

    for name, func in strategies.items():
        t = time_function(func, iterations=100)
        print(f"{name:<50} {t*1e6:<15.2f} {t*1e6/batch_size:<15.2f}")

# Test if accessing .pieces is cheap
print("\n" + "=" * 80)
print("PROTO OBJECT ACCESS OVERHEAD")
print("=" * 80)

text = " ".join(["test"] * 50)

print(f"{'Operation':<50} {'Time (μs)':<15}")
print("-" * 80)

# Create proto once
proto = sp.encode_as_immutable_proto(text)

operations = {
    "Create proto": lambda: sp.encode_as_immutable_proto(text),
    "Create proto + count": lambda: len(sp.encode_as_immutable_proto(text).pieces),
    "Access .pieces (proto cached)": lambda: proto.pieces,
    "Count .pieces (proto cached)": lambda: len(proto.pieces),
}

for name, func in operations.items():
    t = time_function(func, iterations=500)
    print(f"{name:<50} {t*1e6:<15.2f}")

print("\n" + "=" * 80)
print("CONCLUSION")
print("=" * 80)
print("""
The ImmutableSentencePieceText object:
- Has a .pieces attribute that's a list of token objects
- len(proto.pieces) gives the token count
- Similar performance to len(sp.encode(text))

However, the proto object provides rich metadata (begin/end positions, surface form)
which could be useful for certain applications beyond just counting.

For pure token counting: sp.encode() is still the best choice.
For counting + metadata: encode_as_immutable_proto() provides additional value.
""")
