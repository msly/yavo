use std::io::{stdin, Read};
fn nums(f: &[&str]) -> Option<Vec<f64>> {
    f.iter().map(|x| x.parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut total = 0.0;
    let mut best: Option<(String, f64)> = None;
    for (i, l) in inp.lines().enumerate() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let parsed = if f.is_empty() { None } else {
            let need = match f[0] { "circle" => 1, "rect" => 2, "triangle" => 3, _ => 0 };
            if need == 0 || f.len() != need + 1 { None } else { nums(&f[1..]).map(|v| (f[0], v)) }
        };
        let (name, v) = match parsed { Some(x) => x, None => { println!("line {}: cannot parse", i + 1); continue; } };
        let area = match name {
            "circle" => std::f64::consts::PI * v[0] * v[0],
            "rect" => v[0] * v[1],
            _ => {
                let (a, b, c) = (v[0], v[1], v[2]);
                if !(a + b > c && a + c > b && b + c > a) { println!("triangle: invalid"); continue; }
                let s = (a + b + c) / 2.0;
                (s * (s - a) * (s - b) * (s - c)).sqrt()
            }
        };
        println!("{}: {:.2}", name, area);
        total += area;
        if best.as_ref().map_or(true, |b| area > b.1) { best = Some((name.to_string(), area)); }
    }
    println!("total: {:.2}", total);
    println!("largest: {}", best.map_or("none".to_string(), |b| b.0));
}
