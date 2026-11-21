T01 build 1: fail E0404 getCopy needs Copy value type (String)
T01 build 2: fail E0404 same error (my python edit tool was missing; wasted build)
T01 build 3: ok
T01: PASS
T02 build 1: fail E0404 cannot take String out of borrow (if let on owned Option from pop)
T02 build 2: ok
T02: PASS
T03 build 1: ok
T03 build 1: (test) output wrong on i64.min result (my sentinel); T03 build 2: fail U0001 field default values not supported in P0 (cheatsheet shows y: f64 = 0.0)
T03 build 3: ok
T03: PASS
T04 build 1: fail E0020 unescaped "${" in string literal (need "\${")
T04 build 2: ok
T04: PASS
T05 build 1: fail U0001 x3 index assignment v[i] = x not supported (only variables/fields assignable)
T05 build 2: ok
T05: PASS
T06 build 1: ok
T06: PASS
T07 build 1: ok
T07: PASS
T08 build 1: fail E0302 x1,E0402 x3,E0102 x2,E0401 x1 (missing 'of' on str return; moved names; borrow first; move out of field; set while iterating)
T08 build 2: ok
T08: PASS
T09 build 1: fail E0001 x2 cannot compare String with str (k == p)
T09 build 2: ok
T09: PASS
T10 build 1: ok (warning W0001 unused e)
T10: PASS
T11 build 1: fail E0403 x2 tab.push(f(tab)) argument uses borrowed tab
T11 build 2: ok
T11: PASS
T12 build 1: fail E0403 x2 cnt.push(cnt[x]+cnt[y]) argument uses borrowed vec
T12 build 2: ok
T12: PASS
