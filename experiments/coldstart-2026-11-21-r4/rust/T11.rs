use std::fs;

fn read(p: &str) -> Vec<String> {
    match fs::read(p) {
        Ok(b) => String::from_utf8_lossy(&b).lines().map(|s| s.to_string()).collect(),
        Err(_) => {
            eprintln!("error: cannot read {}", p);
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: T11 A B");
        std::process::exit(2);
    }
    let a = read(&args[0]);
    let b = read(&args[1]);
    let (n, m) = (a.len(), b.len());
    let mut l = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            l[i][j] = if a[i] == b[j] { l[i + 1][j + 1] + 1 } else { l[i + 1][j].max(l[i][j + 1]) };
        }
    }
    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    let (mut del, mut add) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push_str(&format!("  {}\n", a[i]));
            i += 1;
            j += 1;
        } else if l[i + 1][j] >= l[i][j + 1] {
            out.push_str(&format!("- {}\n", a[i]));
            del += 1;
            i += 1;
        } else {
            out.push_str(&format!("+ {}\n", b[j]));
            add += 1;
            j += 1;
        }
    }
    while i < n {
        out.push_str(&format!("- {}\n", a[i]));
        del += 1;
        i += 1;
    }
    while j < m {
        out.push_str(&format!("+ {}\n", b[j]));
        add += 1;
        j += 1;
    }
    out.push_str(&format!("summary: -{} +{}\n", del, add));
    print!("{}", out);
    std::process::exit(if del + add == 0 { 0 } else { 1 });
}
