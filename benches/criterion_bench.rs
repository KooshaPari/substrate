use criterion::{black_box, criterion_group, criterion_main, Criterion};

// ---------------------------------------------------------------------------
// SHA-256 benchmarks
// ---------------------------------------------------------------------------

fn bench_sha256(c: &mut Criterion) {
    let mut group = c.benchmark_group("sha256");

    let small = b"hello world";
    let medium: Vec<u8> = (0..1024).map(|i| (i & 0xff) as u8).collect();
    let large: Vec<u8> = (0..65536).map(|i| (i & 0xff) as u8).collect();

    group.bench_function("hash_11B", |b| {
        b.iter(|| gateway::sha256::hash(black_box(small)))
    });
    group.bench_function("hash_1KB", |b| {
        b.iter(|| gateway::sha256::hash(black_box(&medium)))
    });
    group.bench_function("hash_64KB", |b| {
        b.iter(|| gateway::sha256::hash(black_box(&large)))
    });

    // Incremental (streaming) hasher benchmark
    group.bench_function("incremental_64KB", |b| {
        b.iter(|| {
            let mut h = gateway::sha256::Hasher::new();
            for chunk in large.chunks(1024) {
                h.update(black_box(chunk));
            }
            h.finalize()
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// MD5 benchmarks
// ---------------------------------------------------------------------------

fn bench_md5(c: &mut Criterion) {
    let mut group = c.benchmark_group("md5");

    let small = b"hello world";
    let medium: Vec<u8> = (0..1024).map(|i| (i & 0xff) as u8).collect();
    let large: Vec<u8> = (0..65536).map(|i| (i & 0xff) as u8).collect();

    group.bench_function("hash_11B", |b| {
        b.iter(|| gateway::md5::hash(black_box(small)))
    });
    group.bench_function("hash_1KB", |b| {
        b.iter(|| gateway::md5::hash(black_box(&medium)))
    });
    group.bench_function("hash_64KB", |b| {
        b.iter(|| gateway::md5::hash(black_box(&large)))
    });

    // Incremental hasher benchmark
    group.bench_function("incremental_64KB", |b| {
        b.iter(|| {
            let mut h = gateway::md5::Hasher::new();
            for chunk in large.chunks(1024) {
                h.update(black_box(chunk));
            }
            h.finalize()
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// LZ77 compression benchmarks
// ---------------------------------------------------------------------------

fn bench_lz77(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz77");

    // Highly compressible: repeating pattern
    let repetitive: Vec<u8> = b"the quick brown fox jumps over the lazy dog ".repeat(100);
    // Low compressibility: pseudo-random bytes
    let random: Vec<u8> = (0..4096u32)
        .map(|i| ((i.wrapping_mul(2654435761) >> 16) & 0xff) as u8)
        .collect();
    // Mixed: some repetition + some random
    let mut mixed = Vec::new();
    mixed.extend_from_slice(b"LZ77 is the basis for many popular compression formats. ");
    mixed.extend_from_slice(b"DEFLATE uses LZ77 and Huffman coding together. ");
    mixed.extend_from_slice(&random[..2048]);

    group.bench_with_input("compress_repetitive", &repetitive, |b, data| {
        b.iter(|| gateway::lz77::lz77_compress(black_box(data)))
    });
    group.bench_with_input("compress_random_4KB", &random, |b, data| {
        b.iter(|| gateway::lz77::lz77_compress(black_box(data)))
    });
    group.bench_with_input("compress_mixed", &mixed, |b, data| {
        b.iter(|| gateway::lz77::lz77_compress(black_box(data)))
    });

    // Decompress benchmark: compress first, then measure decompression
    let tokens_repetitive = gateway::lz77::lz77_compress(&repetitive);
    group.bench_with_input(
        "decompress_repetitive",
        &tokens_repetitive,
        |b, tokens| {
            b.iter(|| gateway::lz77::lz77_decompress(black_box(tokens)))
        },
    );

    let tokens_random = gateway::lz77::lz77_compress(&random);
    group.bench_with_input("decompress_random", &tokens_random, |b, tokens| {
        b.iter(|| gateway::lz77::lz77_decompress(black_box(tokens)))
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Blowfish cipher benchmarks
// ---------------------------------------------------------------------------

fn bench_blowfish(c: &mut Criterion) {
    let mut group = c.benchmark_group("blowfish");

    let key = b"benchmark-key-for-blowfish-tests!!";

    group.bench_function("new_32B_key", |b| {
        b.iter(|| gateway::blowfish::Blowfish::new(black_box(key)))
    });

    let bf = gateway::blowfish::Blowfish::new(key);

    // Single-block encrypt/decrypt
    group.bench_function("encrypt_block_u32", |b| {
        b.iter(|| bf.encrypt_block_u32(black_box(0x01020304), black_box(0x05060708)))
    });
    group.bench_function("decrypt_block_u32", |b| {
        b.iter(|| bf.decrypt_block_u32(black_box(0xdeadbeef), black_box(0xcafebabe)))
    });

    // Byte-level block encrypt/decrypt
    group.bench_function("encrypt_block_bytes", |b| {
        b.iter(|| bf.encrypt_block_bytes(black_box(b"\x01\x02\x03\x04\x05\x06\x07\x08")))
    });

    // CBC mode: multi-block encrypt/decrypt (128 bytes = 16 blocks)
    let plaintext_128: Vec<u8> = (0..128).map(|i| (i & 0xff) as u8).collect();
    let iv = (0xAABBCCDDu32, 0x11223344u32);

    group.bench_function("encrypt_cbc_128B", |b| {
        b.iter(|| bf.encrypt_cbc(black_box(&plaintext_128), iv))
    });

    let ciphertext_128 = bf.encrypt_cbc(&plaintext_128, iv);
    group.bench_function("decrypt_cbc_128B", |b| {
        b.iter(|| bf.decrypt_cbc(black_box(&ciphertext_128), iv))
    });

    // Larger CBC: 1 KB
    let plaintext_1k: Vec<u8> = (0..1024).map(|i| (i & 0xff) as u8).collect();
    let ciphertext_1k = bf.encrypt_cbc(&plaintext_1k, iv);

    group.bench_function("encrypt_cbc_1KB", |b| {
        b.iter(|| bf.encrypt_cbc(black_box(&plaintext_1k), iv))
    });
    group.bench_function("decrypt_cbc_1KB", |b| {
        b.iter(|| bf.decrypt_cbc(black_box(&ciphertext_1k), iv))
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// ChaCha20 stream cipher benchmarks
// ---------------------------------------------------------------------------

fn bench_chacha20(c: &mut Criterion) {
    let mut group = c.benchmark_group("chacha20");

    let key = [0x42u8; 32];
    let nonce = [0x11u8; 12];

    // Keystream block generation
    group.bench_function("block", |b| {
        b.iter(|| gateway::chacha20::block(black_box(&key), black_box(0), black_box(&nonce)))
    });

    // Encryption of various sizes
    let small = b"hello world";
    let medium: Vec<u8> = (0..1024).map(|i| (i & 0xff) as u8).collect();
    let large: Vec<u8> = (0..65536).map(|i| (i & 0xff) as u8).collect();

    group.bench_function("encrypt_11B", |b| {
        b.iter(|| {
            gateway::chacha20::encrypt(
                black_box(&key),
                0,
                black_box(&nonce),
                black_box(small),
            )
        })
    });
    group.bench_function("encrypt_1KB", |b| {
        b.iter(|| {
            gateway::chacha20::encrypt(
                black_box(&key),
                0,
                black_box(&nonce),
                black_box(&medium),
            )
        })
    });
    group.bench_function("encrypt_64KB", |b| {
        b.iter(|| {
            gateway::chacha20::encrypt(
                black_box(&key),
                0,
                black_box(&nonce),
                black_box(&large),
            )
        })
    });

    // Decrypt (symmetric with encrypt, but benchmarks the path)
    let ciphertext_1k = gateway::chacha20::encrypt(&key, 0, &nonce, &medium);
    group.bench_function("decrypt_1KB", |b| {
        b.iter(|| {
            gateway::chacha20::decrypt(
                black_box(&key),
                0,
                black_box(&nonce),
                black_box(&ciphertext_1k),
            )
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Circuit breaker state transition benchmarks
// ---------------------------------------------------------------------------

fn bench_circuit_breaker(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");

    group.bench_function("new", |b| {
        b.iter(|| gateway::circuit_breaker::CircuitBreaker::new())
    });

    group.bench_function("state_check", |b| {
        let cb = gateway::circuit_breaker::CircuitBreaker::new();
        b.iter(|| black_box(cb.state()))
    });

    group.bench_function("is_open_check", |b| {
        let cb = gateway::circuit_breaker::CircuitBreaker::new();
        b.iter(|| black_box(cb.is_open()))
    });

    // Simulate a sequence of failures to trigger state transitions
    group.bench_function("record_failure_sequence", |b| {
        b.iter_with_setup(
            || gateway::circuit_breaker::CircuitBreaker::new(),
            |mut cb| {
                for _ in 0..10 {
                    cb.record_failure();
                }
            },
        )
    });

    group.bench_function("record_success_sequence", |b| {
        b.iter_with_setup(
            || {
                let mut cb = gateway::circuit_breaker::CircuitBreaker::new();
                // Force into Open state
                for _ in 0..10 {
                    cb.record_failure();
                }
                cb
            },
            |mut cb| {
                for _ in 0..5 {
                    cb.record_success();
                }
            },
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Group & main
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_sha256,
    bench_md5,
    bench_lz77,
    bench_blowfish,
    bench_chacha20,
    bench_circuit_breaker,
);
criterion_main!(benches);
