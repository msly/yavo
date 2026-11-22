use std::collections::{BTreeMap,BTreeSet};
use std::io::{self,Read};
fn joined(values:Vec<&str>)->String{if values.is_empty(){"none".to_string()}else{values.join(" ")}}
fn main(){let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();let mut docs:BTreeMap<String,BTreeSet<String>>=BTreeMap::new();let mut out=String::new();
    for line in s.lines(){let p:Vec<&str>=line.split_whitespace().collect();if p.is_empty(){continue;}
        match p[0]{
            "set"=>{docs.insert(p[1].to_string(),p[2..].iter().map(|t|t.to_string()).collect());}
            "add"=>{docs.entry(p[1].to_string()).or_default().insert(p[2].to_string());}
            "remove"=>{if let Some(tags)=docs.get_mut(p[1]){tags.remove(p[2]);}}
            "drop"=>{docs.remove(p[1]);}
            "tags"=>{if let Some(tags)=docs.get(p[1]){out.push_str(&format!("{}: {}\n",p[1],joined(tags.iter().map(|t|t.as_str()).collect())));}else{out.push_str(&format!("{}: missing\n",p[1]));}}
            "query"=>{let matches=docs.iter().filter(|(_,tags)|p[1..].iter().all(|t|tags.contains(*t))).map(|(d,_)|d.as_str()).collect();out.push_str(&format!("docs: {}\n",joined(matches)));}
            _=>unreachable!()
        }
    }
    let all:BTreeSet<&String>=docs.values().flat_map(|tags|tags.iter()).collect();out.push_str(&format!("total: docs={} tags={}\n",docs.len(),all.len()));print!("{}",out);
}
