use std::io::{self,Read};
fn main(){
    let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();let mut lines=s.lines();let dims:Vec<usize>=lines.next().unwrap().split_whitespace().map(|x|x.parse().unwrap()).collect();let(h,w)=(dims[0],dims[1]);
    let grid:Vec<Vec<u8>>=(0..h).map(|_|lines.next().unwrap().as_bytes().to_vec()).collect();let mut seen=vec![vec![false;w];h];let mut components=Vec::new();
    for r in 0..h{for c in 0..w{if grid[r][c]!=b'#'||seen[r][c]{continue;}let mut area=0;let mut perimeter=0;let(mut r0,mut c0,mut r1,mut c1)=(r,c,r,c);let mut stack=vec![(r,c)];seen[r][c]=true;
        while let Some((y,x))=stack.pop(){area+=1;r0=r0.min(y);c0=c0.min(x);r1=r1.max(y);c1=c1.max(x);
            for (dy,dx) in [(-1isize,0isize),(1,0),(0,-1),(0,1)]{let ny=y as isize+dy;let nx=x as isize+dx;if ny<0||nx<0||ny>=h as isize||nx>=w as isize{perimeter+=1;continue;}let(a,b)=(ny as usize,nx as usize);if grid[a][b]==b'.'{perimeter+=1;}else if !seen[a][b]{seen[a][b]=true;stack.push((a,b));}}
        }
        components.push((area,perimeter,r0,c0,r1,c1));
    }}
    let mut out=format!("components: {}\n",components.len());let mut largest=None;let mut best=0;
    for(i,&(a,p,r0,c0,r1,c1))in components.iter().enumerate(){out.push_str(&format!("{}: area={} perimeter={} box={},{}:{},{}\n",i+1,a,p,r0,c0,r1,c1));if a>best{best=a;largest=Some(i+1);}}
    out.push_str(&format!("largest: {}\n",largest.map(|x|x.to_string()).unwrap_or_else(||"none".to_string())));print!("{}",out);
}
