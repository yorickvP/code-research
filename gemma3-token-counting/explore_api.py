#!/usr/bin/env python3
"""Explore SentencePiece API to find optimization opportunities."""

import sentencepiece as spm

# Load the tokenizer
sp = spm.SentencePieceProcessor()
sp.load('gemma3_cleaned_262144_v2.spiece.model')

# Test text
test_text = "Hello world! This is a test of the Gemma3 tokenizer."

print("=" * 60)
print("SentencePiece API Exploration")
print("=" * 60)

# List available methods
print("\nAvailable methods:")
methods = [m for m in dir(sp) if not m.startswith('_')]
for method in methods:
    print(f"  - {method}")

print("\n" + "=" * 60)
print("Testing different encoding methods:")
print("=" * 60)

# Test 1: encode (returns list of token IDs)
result1 = sp.encode(test_text)
print(f"\n1. encode(): {type(result1)}")
print(f"   Result: {result1}")
print(f"   Token count: {len(result1)}")

# Test 2: encode_as_pieces (returns list of token strings)
result2 = sp.encode_as_pieces(test_text)
print(f"\n2. encode_as_pieces(): {type(result2)}")
print(f"   Result: {result2}")
print(f"   Token count: {len(result2)}")

# Test 3: encode_as_ids (same as encode?)
result3 = sp.encode_as_ids(test_text)
print(f"\n3. encode_as_ids(): {type(result3)}")
print(f"   Result: {result3}")
print(f"   Token count: {len(result3)}")

# Test 4: Check if there's a get_piece_size or similar
print("\n" + "=" * 60)
print("Checking for count/size methods:")
print("=" * 60)

# Look for methods that might give us token count directly
count_methods = [m for m in methods if 'count' in m.lower() or 'size' in m.lower() or 'len' in m.lower()]
print(f"Count/size related methods: {count_methods}")

# Test batching capabilities
print("\n" + "=" * 60)
print("Testing batch encoding:")
print("=" * 60)

texts = [
    "First text",
    "Second text here",
    "Third text is longer than the others"
]

try:
    batch_result = sp.encode(texts)
    print(f"Batch encode result type: {type(batch_result)}")
    print(f"Batch encode result: {batch_result}")
except Exception as e:
    print(f"Batch encoding with single call failed: {e}")
    print("Need to iterate manually")

# Check vocabulary size
print(f"\nVocabulary size: {sp.get_piece_size()}")
print(f"BOS ID: {sp.bos_id()}")
print(f"EOS ID: {sp.eos_id()}")
print(f"PAD ID: {sp.pad_id()}")
print(f"UNK ID: {sp.unk_id()}")
