use std::io::{self,Read};
fn parse(line:&str)->Option<Vec<String>> {
    let b=line.as_bytes(); let mut i=0; let mut fields=Vec::new();
    loop {
        let mut value=Vec::new();
        if i<b.len() && b[i]==b'"' {
            i+=1; let mut closed=false;
            while i<b.len() {
                if b[i]==b'"' {
                    if i+1<b.len() && b[i+1]==b'"' {value.push(b'"');i+=2;}
                    else {i+=1;closed=true;break;}
                } else {value.push(b[i]);i+=1;}
            }
            if !closed || (i<b.len() && b[i]!=b',') {return None;}
        } else {
            while i<b.len() && b[i]!=b',' {if b[i]==b'"' {return None;}value.push(b[i]);i+=1;}
        }
        fields.push(String::from_utf8(value).unwrap());
        if i==b.len() {return Some(fields);}
        i+=1;
    }
}
fn main(){
    let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();let mut out=String::new();
    for (k,line) in s.lines().enumerate(){
        if let Some(fields)=parse(line){out.push_str(&format!("{}: {}\n",k+1,fields.len()));for f in fields {out.push_str(&format!("{}:{}\n",f.len(),f));}}
        else {out.push_str(&format!("{}: invalid\n",k+1));}
    }
    print!("{}",out);
}
