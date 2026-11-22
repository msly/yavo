use std::cmp::Ordering;
use std::io::{self,Read};
struct Version<'a>{original:&'a str,core:[i64;3],pre:Option<Vec<&'a str>>}
fn identifiers(s:&str)->Option<Vec<&str>>{
    let ids:Vec<&str>=s.split('.').collect();if ids.iter().any(|id|id.is_empty()||id.len()>32||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')){None}else{Some(ids)}
}
fn number(s:&str)->Option<i64>{if s.is_empty()||!s.bytes().all(|b|b.is_ascii_digit()){return None;}let mut n=0i64;for b in s.bytes(){n=n*10+(b-b'0')as i64;if n>1000000000{return None;}}Some(n)}
fn parse(s:&str)->Option<Version<'_>>{
    let body=if let Some((body,build))=s.split_once('+'){identifiers(build)?;body}else{s};
    let(core,pre)=if let Some((core,pre))=body.split_once('-'){(core,Some(identifiers(pre)?))}else{(body,None)};
    let parts:Vec<&str>=core.split('.').collect();if parts.len()!=3{return None;}
    Some(Version{original:s,core:[number(parts[0])?,number(parts[1])?,number(parts[2])?],pre})
}
fn compare_ids(a:&str,b:&str)->Ordering{
    let an=a.bytes().all(|c|c.is_ascii_digit());let bn=b.bytes().all(|c|c.is_ascii_digit());
    match(an,bn){(true,true)=>{let a=a.trim_start_matches('0');let b=b.trim_start_matches('0');a.len().cmp(&b.len()).then_with(||a.cmp(b))},(true,false)=>Ordering::Less,(false,true)=>Ordering::Greater,(false,false)=>a.cmp(b)}
}
fn compare(a:&Version<'_>,b:&Version<'_>)->Ordering{
    let core=a.core.cmp(&b.core);if core!=Ordering::Equal{return core;}
    match(&a.pre,&b.pre){(None,None)=>Ordering::Equal,(Some(_),None)=>Ordering::Less,(None,Some(_))=>Ordering::Greater,(Some(a),Some(b))=>{for(x,y)in a.iter().zip(b.iter()){let c=compare_ids(x,y);if c!=Ordering::Equal{return c;}}a.len().cmp(&b.len())}}
}
fn main(){let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();let mut valid=Vec::new();let mut invalid=0;let mut out=String::new();for(k,line)in s.lines().enumerate(){if let Some(v)=parse(line){valid.push(v);}else{invalid+=1;out.push_str(&format!("line {}: invalid\n",k+1));}}valid.sort_by(compare);for v in &valid{out.push_str(v.original);out.push('\n');}out.push_str(&format!("valid: {} invalid: {}\n",valid.len(),invalid));print!("{}",out);}
