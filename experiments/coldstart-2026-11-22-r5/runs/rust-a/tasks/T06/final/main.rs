use std::io::{self,Read};
use std::thread;
fn encode(segment:Vec<u8>)->Vec<(u8,usize)>{
    let mut runs:Vec<(u8,usize)>=Vec::new();
    for byte in segment{if let Some(last)=runs.last_mut(){if last.0==byte{last.1+=1;continue;}}runs.push((byte,1));}runs
}
fn main(){
    let mut input=Vec::new();io::stdin().read_to_end(&mut input).unwrap();input.pop();let n=input.len();let mut handles=Vec::new();let mut start=0;
    for i in 0..3{let len=n/3+if i<n%3{1}else{0};let segment=input[start..start+len].to_vec();start+=len;handles.push(thread::spawn(move||encode(segment)));}
    let mut combined:Vec<(u8,usize)>=Vec::new();
    for handle in handles{for (byte,count) in handle.join().unwrap(){if let Some(last)=combined.last_mut(){if last.0==byte{last.1+=count;continue;}}combined.push((byte,count));}}
    let mut out=String::new();for &(byte,count) in &combined{out.push_str(&format!("{} {}\n",byte,count));}out.push_str(&format!("runs: {} chars: {}\n",combined.len(),n));print!("{}",out);
}
