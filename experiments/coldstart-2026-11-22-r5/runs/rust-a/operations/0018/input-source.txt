fn unit(s:&str)->Option<i64>{match s{"mm"=>Some(1),"cm"=>Some(10),"m"=>Some(1000),"km"=>Some(1000000),_=>None}}
fn amount(s:&str)->Option<i64>{
    if s.is_empty()||s.len()>64{return None;}
    let negative=s.starts_with('-');let t=if negative{&s[1..]}else{s};
    let mut pieces=t.split('.');let whole=pieces.next()?;let fraction=pieces.next();if pieces.next().is_some(){return None;}
    if whole.is_empty()||!whole.bytes().all(|b|b.is_ascii_digit()){return None;}
    let mut n=0i64;for b in whole.bytes(){n=n*10+(b-b'0') as i64;if n>1000000{return None;}}
    let mut f=0i64;
    if let Some(v)=fraction{if v.is_empty()||v.len()>3||!v.bytes().all(|b|b.is_ascii_digit()){return None;}for b in v.bytes(){f=f*10+(b-b'0') as i64;}for _ in v.len()..3{f*=10;}}
    if n==1000000&&f!=0{return None;}
    let result=n*1000+f;Some(if negative{-result}else{result})
}
fn gcd(mut a:i64,mut b:i64)->i64{while b!=0{let r=a%b;a=b;b=r;}a}
fn convert(line:&str)->Option<String>{
    let p:Vec<&str>=line.split(|c|c==' '||c=='\t').filter(|s|!s.is_empty()).collect();
    if p.len()!=3{return None;}
    let mut numerator=amount(p[0])?*unit(p[1])?;let mut denominator=1000*unit(p[2])?;
    let g=gcd(numerator.abs(),denominator);numerator/=g;denominator/=g;
    if denominator==1{Some(format!("{} {}",numerator,p[2]))}else{Some(format!("{}/{} {}",numerator,denominator,p[2]))}
}
pub fn process(input:&str)->String{
    let mut out=String::new();for(k,line)in input.lines().enumerate(){let value=convert(line).unwrap_or_else(||"invalid".to_string());out.push_str(&format!("{}: {}\n",k+1,value));}out
}
