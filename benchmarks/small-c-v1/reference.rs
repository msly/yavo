// Independent correctness oracle. Its running time is not a benchmark result.
use std::collections::VecDeque;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    assert_eq!(a.len(), 5);
    let n: usize = a[2].parse().unwrap();
    let work: usize = a[3].parse().unwrap();
    let seed: i64 = a[4].parse().unwrap();
    let answer: i64 = match a[1].as_str() {
        "integer_recurrence" => {
            let mut x = seed;
            let mut sum = 0;
            for _ in 0..work {
                for _ in 0..n {
                    x = (x * 1664525 + 1013904223) % 2147483647;
                    sum = (sum + x) % 1000000007;
                }
            }
            sum
        }
        "vec_transform" => {
            let mut v: Vec<i64> = (0..n).map(|i| (i as i64 * 17 + seed) % 65521).collect();
            for round in 0..work {
                for value in &mut v { *value = (*value * 3 + round as i64) % 65521; }
            }
            v.into_iter().sum()
        }
        "mutref_scatter" => {
            let mut v: Vec<i64> = (0..n).map(|i| (i as i64 * 17 + seed) % 65521).collect();
            let mut x = seed;
            for _ in 0..work {
                x = (x * 48271) % 2147483647;
                let value = &mut v[x as usize % n];
                *value = (*value + 1) % 65521;
            }
            v.into_iter().sum()
        }
        "deque_ring" => {
            let mut q: VecDeque<i64> = (0..n).map(|i| (i as i64 * 17 + seed) % 65521).collect();
            let mut sum = 0;
            for _ in 0..work {
                let value = q.pop_front().unwrap_or(-1);
                sum += value;
                q.push_back((value + 1) % 65521);
            }
            sum + q.into_iter().sum::<i64>()
        }
        other => panic!("unknown case {other}")
    };
    println!("{answer}");
}
