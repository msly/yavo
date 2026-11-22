use std::collections::HashMap;

pub struct Config {
    map: HashMap<(String, String), String>,
}

enum Fail {
    Missing(String),
    Cycle,
}

fn split_ref(q: &str) -> (String, String) {
    match q.find('.') {
        Some(p) => (q[..p].to_string(), q[p + 1..].to_string()),
        None => (String::new(), q.to_string()),
    }
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
            } else if let Some(p) = line.find('=') {
                let key = line[..p].trim();
                if key.is_empty() {
                    return Err(i + 1);
                }
                let val = line[p + 1..].trim();
                map.insert((section.clone(), key.to_string()), val.to_string());
            } else {
                return Err(i + 1);
            }
        }
        Ok(Config { map })
    }

    fn expand(&self, sk: &(String, String), stack: &mut Vec<(String, String)>) -> Result<String, Fail> {
        let val = self.map.get(sk).unwrap();
        stack.push(sk.clone());
        let mut out = String::new();
        let mut rest: &str = val;
        while let Some(p) = rest.find("${") {
            out.push_str(&rest[..p]);
            let after = &rest[p + 2..];
            match after.find('}') {
                None => {
                    out.push_str(&rest[p..]);
                    rest = "";
                    break;
                }
                Some(e) => {
                    let r = &after[..e];
                    let k = split_ref(r);
                    if !self.map.contains_key(&k) {
                        return Err(Fail::Missing(r.to_string()));
                    }
                    if stack.contains(&k) {
                        return Err(Fail::Cycle);
                    }
                    out.push_str(&self.expand(&k, stack)?);
                    rest = &after[e + 1..];
                }
            }
        }
        out.push_str(rest);
        stack.pop();
        Ok(out)
    }

    pub fn query(&self, q: &str) -> String {
        let k = split_ref(q);
        if !self.map.contains_key(&k) {
            return format!("{}: not found", q);
        }
        let mut stack = Vec::new();
        match self.expand(&k, &mut stack) {
            Ok(v) => format!("{} = {}", q, v),
            Err(Fail::Missing(r)) => format!("{}: missing {}", q, r),
            Err(Fail::Cycle) => format!("{}: cycle", q),
        }
    }
}
