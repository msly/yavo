use std::io::{self,Read};
struct Entry{key:String,value:i64,expires:i64}
fn keys(cache:&[Entry])->String{if cache.is_empty(){"keys: none\n".to_string()}else{format!("keys: {}\n",cache.iter().map(|e|e.key.as_str()).collect::<Vec<_>>().join(" "))}}
fn main(){let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();let mut lines=s.lines();let capacity:usize=lines.next().unwrap().trim().parse().unwrap();let mut cache:Vec<Entry>=Vec::new();let mut out=String::new();
    for line in lines{let p:Vec<&str>=line.split_whitespace().collect();if p.is_empty(){continue;}let t:i64=p[0].parse().unwrap();cache.retain(|e|e.expires>t);
        match p[1]{
            "put"=>{let value:i64=p[3].parse().unwrap();let ttl:i64=p[4].parse().unwrap();cache.retain(|e|e.key!=p[2]);if ttl>0&&capacity>0{cache.insert(0,Entry{key:p[2].to_string(),value,expires:t+ttl});cache.truncate(capacity);}}
            "get"=>{if let Some(i)=cache.iter().position(|e|e.key==p[2]){let entry=cache.remove(i);out.push_str(&format!("{}: {}\n",entry.key,entry.value));cache.insert(0,entry);}else{out.push_str(&format!("{}: missing\n",p[2]));}}
            "keys"=>out.push_str(&keys(&cache)),
            _=>unreachable!()
        }
    }
    out.push_str(&keys(&cache));print!("{}",out);
}
