//! Linear systems over GF(2) with up to 128 unknowns, one u128 per row.

/// Every solution is `particular` XOR any combination of the `kernel` vectors.
#[derive(Debug)]
pub struct Solution {
    pub particular: u128,
    pub kernel: Vec<u128>,
}

/// Solve rows of (coefficients, right-hand side). None if inconsistent.
pub fn solve(rows: &[(u128, bool)], nvars: usize) -> Option<Solution> {
    assert!(nvars <= 128);
    let mut rows = rows.to_vec();
    let mut pivots = Vec::new(); // (column, row)
    let mut next = 0;
    for col in 0..nvars {
        let bit = 1u128 << col;
        let Some(found) = (next..rows.len()).find(|&i| rows[i].0 & bit != 0) else {
            continue;
        };
        rows.swap(next, found);
        let (pc, pr) = rows[next];
        for (i, row) in rows.iter_mut().enumerate() {
            if i != next && row.0 & bit != 0 {
                row.0 ^= pc;
                row.1 ^= pr;
            }
        }
        pivots.push((col, next));
        next += 1;
    }
    // Rows past the last pivot are now all-zero on the left: 0 = 1 means no solution.
    if rows[next..].iter().any(|&(_, rhs)| rhs) {
        return None;
    }
    let mut particular = 0u128;
    let mut pivot_cols = 0u128;
    for &(col, row) in &pivots {
        pivot_cols |= 1 << col;
        if rows[row].1 {
            particular |= 1 << col;
        }
    }
    let kernel = (0..nvars)
        .filter(|free| pivot_cols >> free & 1 == 0)
        .map(|free| {
            let mut v = 1u128 << free;
            for &(col, row) in &pivots {
                if rows[row].0 >> free & 1 == 1 {
                    v |= 1 << col;
                }
            }
            v
        })
        .collect();
    Some(Solution { particular, kernel })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn satisfies(rows: &[(u128, bool)], x: u128) -> bool {
        rows.iter().all(|&(c, rhs)| ((c & x).count_ones() % 2 == 1) == rhs)
    }

    #[test]
    fn unique_solution() {
        // a + b = 1, b = 1, a + c = 0  =>  a = 0, b = 1, c = 0
        let rows = [(0b011, true), (0b010, true), (0b101, false)];
        let s = solve(&rows, 3).unwrap();
        assert_eq!(s.particular, 0b010);
        assert!(s.kernel.is_empty());
    }

    #[test]
    fn inconsistent_system() {
        let rows = [(0b11, true), (0b11, false)];
        assert!(solve(&rows, 2).is_none());
    }

    #[test]
    fn underdetermined_system_has_a_kernel_that_works() {
        // a + b + c = 1 alone: two free variables.
        let rows = [(0b111, true)];
        let s = solve(&rows, 3).unwrap();
        assert_eq!(s.kernel.len(), 2);
        assert!(satisfies(&rows, s.particular));
        for k in &s.kernel {
            assert!(satisfies(&rows, s.particular ^ k));
        }
    }

    #[test]
    fn random_full_width_systems() {
        let mut state = 0x1234_5678_9abc_def1u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..50 {
            let secret = u128::from(next()) | (u128::from(next()) << 64);
            let rows: Vec<(u128, bool)> = (0..160)
                .map(|_| {
                    let c = u128::from(next()) | (u128::from(next()) << 64);
                    (c, (c & secret).count_ones() % 2 == 1)
                })
                .collect();
            let s = solve(&rows, 128).unwrap();
            assert!(satisfies(&rows, s.particular));
            for k in &s.kernel {
                assert!(satisfies(&rows, s.particular ^ k));
            }
            if s.kernel.is_empty() {
                assert_eq!(s.particular, secret);
            }
        }
    }
}
