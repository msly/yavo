use std::io::{self,Read};
fn main(){
    let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();
    let mut intervals:Vec<(i64,i64)>=Vec::new();let mut out=String::new();
    for line in s.lines(){let p:Vec<&str>=line.split_whitespace().collect();if p.is_empty(){continue;}
        match p[0]{
            "add"=>{let l=p[1].parse().unwrap();let r=p[2].parse().unwrap();intervals.push((l,r));intervals.sort();let mut merged:Vec<(i64,i64)>=Vec::new();for &(a,b) in &intervals{if let Some(last)=merged.last_mut(){if a<=last.1+1{last.1=last.1.max(b);continue;}}merged.push((a,b));}intervals=merged;}
            "del"=>{let l:i64=p[1].parse().unwrap();let r:i64=p[2].parse().unwrap();let mut next=Vec::new();for &(a,b) in &intervals{if b<l||a>r{next.push((a,b));}else{if a<l{next.push((a,l-1));}if b>r{next.push((r+1,b));}}}intervals=next;}
            "has"=>{let x:i64=p[1].parse().unwrap();out.push_str(if intervals.iter().any(|&(a,b)|a<=x&&x<=b){"yes\n"}else{"no\n"});}
            "show"=>{if intervals.is_empty(){out.push_str("empty\n");}else{out.push_str(&intervals.iter().map(|(a,b)|format!("{}:{}",a,b)).collect::<Vec<_>>().join(" "));out.push('\n');}}
            _=>unreachable!()
        }
    }
    let count:i64=intervals.iter().map(|(a,b)|b-a+1).sum();out.push_str(&format!("count: {}\n",count));print!("{}",out);
}
