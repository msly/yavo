use std::io::{self,Read};
enum Token{Literal(u8),Any,Star}
fn matches(pattern:&str,text:&str)->Option<bool>{
    let p=pattern.as_bytes();let t=text.as_bytes();let mut tokens=Vec::new();let mut i=0;
    while i<p.len(){match p[i]{b'\\'=>{i+=1;if i==p.len(){return None;}tokens.push(Token::Literal(p[i]));},b'?'=>tokens.push(Token::Any),b'*'=>tokens.push(Token::Star),b=>tokens.push(Token::Literal(b))}i+=1;}
    let mut dp=vec![false;t.len()+1];dp[0]=true;
    for token in tokens{let mut next=vec![false;t.len()+1];match token{Token::Star=>{next[0]=dp[0];for j in 1..=t.len(){next[j]=dp[j]||next[j-1];}},Token::Any=>{for j in 1..=t.len(){next[j]=dp[j-1];}},Token::Literal(b)=>{for j in 1..=t.len(){next[j]=dp[j-1]&&t[j-1]==b;}}}dp=next;}
    Some(dp[t.len()])
}
fn main(){let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();let lines:Vec<&str>=s.lines().collect();let mut out=String::new();for(k,pair)in lines.chunks(2).enumerate(){let result=match matches(pair[0],pair[1]){Some(true)=>"yes",Some(false)=>"no",None=>"invalid"};out.push_str(&format!("{}: {}\n",k+1,result));}print!("{}",out);}
