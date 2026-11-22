use std::collections::BTreeMap;
use std::io::{self, Read};
fn pay(m: &mut BTreeMap<String,i64>, from: &str, to: &str, amount: i64) -> Result<(),String> {
    let balance = match m.get(from) { Some(v) => *v, None => return Err(format!("unknown {}",from)) };
    if !m.contains_key(to) { return Err(format!("unknown {}",to)); }
    if balance < amount { return Err(format!("funds {}",from)); }
    if from != to { *m.get_mut(from).unwrap() -= amount; *m.get_mut(to).unwrap() += amount; }
    Ok(())
}
fn main() {
    let mut s=String::new(); io::stdin().read_to_string(&mut s).unwrap();
    let mut lines=s.lines(); let mut m=BTreeMap::new(); let mut out=String::new();
    while let Some(line)=lines.next() {
        let p: Vec<&str> = line.split_whitespace().collect(); if p.is_empty() {continue;}
        match p[0] {
            "account" => { if m.contains_key(p[1]) {out.push_str(&format!("error: duplicate {}\n",p[1]));} else {m.insert(p[1].to_string(),p[2].parse::<i64>().unwrap());} }
            "pay" => { match pay(&mut m,p[1],p[2],p[3].parse().unwrap()) {Ok(())=>out.push_str("paid\n"),Err(e)=>out.push_str(&format!("error: {}\n",e))} }
            "balance" => { match m.get(p[1]) {Some(v)=>out.push_str(&format!("{}: {}\n",p[1],v)),None=>out.push_str(&format!("error: unknown {}\n",p[1]))} }
            "batch" => {
                let k:usize=p[1].parse().unwrap(); let mut temp=m.clone(); let mut failure=None;
                for j in 1..=k {let row:Vec<&str>=lines.next().unwrap().split_whitespace().collect(); if failure.is_none() {if let Err(e)=pay(&mut temp,row[0],row[1],row[2].parse().unwrap()) {failure=Some(format!("batch: row {} {}\n",j,e));}}}
                if let Some(e)=failure {out.push_str(&e);} else {m=temp;out.push_str("batch: ok\n");}
            }
            _=>unreachable!()
        }
    }
    let mut total=0i64;
    for (name,balance) in m {out.push_str(&format!("{}={}\n",name,balance));total+=balance;}
    out.push_str(&format!("total: {}\n",total));print!("{}",out);
}
