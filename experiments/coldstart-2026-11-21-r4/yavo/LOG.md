T01 build 1: ok
T02 build 1: ok
T01: PASS
T02: PASS
T03 build 1: fail E0102 cannot assign to self while t (a let-bound borrow of self.toks[self.i]) still used
T03 build 2: ok
T03: PASS
T04 build 1: fail E0020 literal "${" in string needs escaping as "\${" (cheat sheet documents it; I forgot)
T04 build 2: ok
T04: PASS
T05 build 1: fail U0001 push on indexed element (dependents[pi].push(i)) only supported on a variable
T05 build 2: ok
T05: PASS
T06 build 1: ok
T06: PASS
T07 build 1: ok
T07: PASS
T08 build 1: fail 7 errors (check): E0302 missing 'of' on -> str; E0402 use of moved names (let saved = names); E0102 first borrows names; E0401 move String out of field; E0102 modify map during for
T08 build 2: ok
T08: PASS
T09 build 1: ok
T09: PASS
T10 build 1: ok
T10: PASS
T11 build 1: ok
T11: PASS
T12 build 1: ok
T12: PASS
