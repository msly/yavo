# API 主题：modules

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 378–409 行

### 多个源文件

入口文件所在目录是项目根。一个 `.yv` 文件就是一个模块：`use geom` 加载同目录的 `geom.yv`，`use util/text` 加载 `util/text.yv`，`use util/{a, b}` 同时加载两个。只需要构建入口文件：`yavo build main.yv -o app` 会自动找到所有被 `use` 的文件。

```
// geom.yv
pub struct Rect {                     // 顶层的函数和类型要写 pub 才能被别的文件用
    w: int
    h: int
}
pub fn area(r: Rect) -> int {
    return r.w * r.h
}
fn helper() -> int {                  // 不写 pub：只在本文件可见
    return 0
}
```

```
// main.yv
use geom

fn main() {
    let r = Rect{w: 2, h: 3}          // 公开类型可以直接用短名，也可以写 geom.Rect
    println(geom.area(r))             // 函数必须写模块名限定
}
```

- 公开类型的字段和方法都可以访问（还没有字段级私有）。导入只对写它的文件有效，不会传递；每个文件各自写 `use std/...`。互相导入是允许的。
- 两个模块有同名类型时必须写 `a.Point`、`b.Point`。找不到或未导出的名字报 `E0010`。
- `yavo fix` 目前只处理单文件程序；多文件项目报 `U0001` 并且不改文件。

