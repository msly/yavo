const T: [(u32, &str); 13] = [(1000,"M"),(900,"CM"),(500,"D"),(400,"CD"),(100,"C"),(90,"XC"),(50,"L"),(40,"XL"),(10,"X"),(9,"IX"),(5,"V"),(4,"IV"),(1,"I")];
fn to_roman(mut n: u32) -> String {
    let mut s = String::new();
    for (v, r) in T.iter() { while n >= *v { s.push_str(r); n -= v; } }
    s
}
fn from_roman(s: &str) -> Option<u32> {
    if s.is_empty() || !s.chars().all(|c| "IVXLCDM".contains(c)) { return None; }
    let val = |c: char| match c { 'I'=>1,'V'=>5,'X'=>10,'L'=>50,'C'=>100,'D'=>500,_=>1000 };
    let cs: Vec<u32> = s.chars().map(val).collect();
    let mut t = 0i64;
    for i in 0..cs.len() {
        if i + 1 < cs.len() && cs[i] < cs[i + 1] { t -= cs[i] as i64; } else { t += cs[i] as i64; }
    }
    if t < 1 || t > 3999 { return None; }
    if to_roman(t as u32) == s { Some(t as u32) } else { None }
}
fn main() {
    for a in std::env::args().skip(1) {
        if !a.is_empty() && a.bytes().all(|c| c.is_ascii_digit()) {
            match a.parse::<u32>() {
                Ok(n) if (1..=3999).contains(&n) => println!("{} -> {}", a, to_roman(n)),
                _ => println!("{}: invalid", a),
            }
        } else {
            match from_roman(&a) { Some(n) => println!("{} -> {}", a, n), None => println!("{}: invalid", a) }
        }
    }
}
