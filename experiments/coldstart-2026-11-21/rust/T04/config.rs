use std::collections::HashMap;

pub struct Config {
    map: HashMap<String, String>,
}

pub enum Fail {
    Missing(String),
    Cycle,
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, usize> {
        let mut map = HashMap::new();
        let mut section = String::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let name = line[1..line.len() - 1].trim();
                if name.is_empty() {
                    return Err(i + 1);
                }
                section = name.to_string();
                continue;
            }
            if let Some(p) = line.find('=') {
                let key = line[..p].trim();
                let val = line[p + 1..].trim();
                if key.is_empty() {
                    return Err(i + 1);
                }
                let full = if section.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", section, key)
                };
                map.insert(full, val.to_string());
                continue;
            }
            return Err(i + 1);
        }
        Ok(Config { map })
    }

    pub fn has(&self, q: &str) -> bool {
        self.map.contains_key(q)
    }

    pub fn expand(&self, q: &str) -> Result<String, Fail> {
        let mut stack = Vec::new();
        self.expand_key(q, &mut stack)
    }

    fn expand_key(&self, key: &str, stack: &mut Vec<String>) -> Result<String, Fail> {
        let val = match self.map.get(key) {
            Some(v) => v,
            None => return Err(Fail::Missing(key.to_string())),
        };
        if stack.iter().any(|k| k == key) {
            return Err(Fail::Cycle);
        }
        stack.push(key.to_string());
        let mut out = String::new();
        let mut rest = val.as_str();
        loop {
            match rest.find("${") {
                None => {
                    out.push_str(rest);
                    break;
                }
                Some(s) => {
                    out.push_str(&rest[..s]);
                    let after = &rest[s + 2..];
                    match after.find('}') {
                        None => {
                            out.push_str(&rest[s..]);
                            break;
                        }
                        Some(e) => {
                            out.push_str(&self.expand_key(&after[..e], stack)?);
                            rest = &after[e + 1..];
                        }
                    }
                }
            }
        }
        stack.pop();
        Ok(out)
    }
}
