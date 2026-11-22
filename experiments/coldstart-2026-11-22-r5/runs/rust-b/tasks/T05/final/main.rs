use std::io::{self, Read};
use std::fmt::Write;
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut tokens = input.split_whitespace();
    let h: usize = tokens.next().unwrap().parse().unwrap();
    let w: usize = tokens.next().unwrap().parse().unwrap();
    let grid: Vec<Vec<u8>> = (0..h).map(|_| tokens.next().unwrap().as_bytes().to_vec()).collect();
    let mut seen = vec![vec![false; w]; h];
    let mut components = Vec::new();
    for row in 0..h {
        for col in 0..w {
            if grid[row][col] != b'#' || seen[row][col] { continue; }
            let mut stack = vec![(row, col)];
            seen[row][col] = true;
            let (mut area, mut perimeter) = (0usize, 0usize);
            let (mut r0, mut c0, mut r1, mut c1) = (row, col, row, col);
            while let Some((r, c)) = stack.pop() {
                area += 1;
                r0 = r0.min(r); c0 = c0.min(c); r1 = r1.max(r); c1 = c1.max(c);
                for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                    let nr = r as isize + dr;
                    let nc = c as isize + dc;
                    if nr < 0 || nc < 0 || nr >= h as isize || nc >= w as isize { perimeter += 1; continue; }
                    let (nr, nc) = (nr as usize, nc as usize);
                    if grid[nr][nc] != b'#' { perimeter += 1; }
                    else if !seen[nr][nc] { seen[nr][nc] = true; stack.push((nr, nc)); }
                }
            }
            components.push((area, perimeter, r0, c0, r1, c1));
        }
    }
    let mut output = String::new();
    writeln!(output, "components: {}", components.len()).unwrap();
    let mut largest = None;
    let mut max_area = 0;
    for (i, &(area, perimeter, r0, c0, r1, c1)) in components.iter().enumerate() {
        writeln!(output, "{}: area={} perimeter={} box={},{}:{},{}", i + 1, area, perimeter, r0, c0, r1, c1).unwrap();
        if area > max_area { max_area = area; largest = Some(i + 1); }
    }
    match largest { Some(k) => writeln!(output, "largest: {}", k).unwrap(), None => writeln!(output, "largest: none").unwrap() }
    print!("{}", output);
}
