pub struct Course {
    pub name: String,
    pub credit: u64,
    pub prereq: Vec<String>,
}

pub fn parse(text: &str) -> Result<Vec<Course>, String> {
    let mut v = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        let credit = if f.len() >= 2 && f[1].bytes().all(|c| c.is_ascii_digit()) {
            f[1].parse::<u64>().ok().filter(|&c| c > 0)
        } else {
            None
        };
        match credit {
            Some(c) => v.push(Course {
                name: f[0].to_string(),
                credit: c,
                prereq: f[2..].iter().map(|s| s.to_string()).collect(),
            }),
            None => return Err(format!("line {}: invalid", i + 1)),
        }
    }
    Ok(v)
}
