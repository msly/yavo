pub struct Course {
    pub name: String,
    pub credits: u64,
    pub prereqs: Vec<String>,
}

pub fn parse(text: &str, cap: u64) -> Result<Vec<Course>, String> {
    let mut list: Vec<Course> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        let credits = if f.len() >= 2 && f[1].bytes().all(|c| c.is_ascii_digit()) {
            f[1].parse::<u64>().ok().filter(|&c| c > 0)
        } else {
            None
        };
        match credits {
            Some(c) => list.push(Course {
                name: f[0].to_string(),
                credits: c,
                prereqs: f[2..].iter().map(|s| s.to_string()).collect(),
            }),
            None => return Err(format!("line {}: invalid", i + 1)),
        }
    }
    for (i, c) in list.iter().enumerate() {
        if list[..i].iter().any(|d| d.name == c.name) {
            return Err(format!("duplicate {}", c.name));
        }
    }
    for c in &list {
        for p in &c.prereqs {
            if !list.iter().any(|d| &d.name == p) {
                return Err(format!("unknown {}", p));
            }
        }
    }
    for c in &list {
        if c.credits > cap {
            return Err(format!("too big {}", c.name));
        }
    }
    Ok(list)
}
