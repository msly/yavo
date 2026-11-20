#!/bin/bash
# usage: cs2_verify.sh ARM  (ARM = yavo | rust); prints one PASS/FAIL line per check
A=$1; W=/tmp/cs2/$A/work; T=/data1/workspace/github/workspace/yavo/experiments/coldstart-2026-11-20/tasks; O=/tmp/cs2v_$A; rm -rf $O; mkdir -p $O
for n in 01 02 03 04 05 06 07 08 09 10; do
  if [ $A = yavo ]; then YAVO_ALLOC_CHECK=1 /data1/workspace/github/workspace/yavo/compiler/target/release/yavo build $W/T$n.yv -o $O/T$n >/dev/null 2>$O/T$n.build || { echo "T$n BUILD FAIL"; continue; }
  else $HOME/.cargo/bin/rustc --edition 2021 -O -C linker=$HOME/.yavo/bin/rust-linker $W/T$n.rs -o $O/T$n 2>$O/T$n.build || { echo "T$n BUILD FAIL"; continue; }; fi
done
chk(){ if [ "$2" = 0 ]; then echo "$1 PASS"; else echo "$1 FAIL"; fi; }
cd $T
$O/T01 < T01.in | cmp -s - T01.out; chk T01 $?
$O/T02 < T02.in | cmp -s - T02.out; chk T02 $?
$O/T03 < T03.in | cmp -s - T03.out; chk T03 $?
$O/T04 < T04a.in | cmp -s - T04a.out; chk T04a $?
o=$($O/T04 < T04b.in 2>$O/e); rc=$?; [ -z "$o" ] && [ $rc = 1 ] && cmp -s $O/e T04b.err; chk T04b $?
o=$($O/T04 < T04c.in 2>$O/e); rc=$?; [ -z "$o" ] && [ $rc = 1 ] && cmp -s $O/e T04c.err; chk T04c $?
$O/T05 < T05a.in > $O/o; rc=$?; [ $rc = 0 ] && cmp -s $O/o T05a.out; chk T05a $?
$O/T05 < T05b.in > $O/o; rc=$?; [ $rc = 1 ] && cmp -s $O/o T05b.out; chk T05b $?
$O/T06 encode < T06.in | cmp -s - T06.enc; chk T06enc $?
$O/T06 decode < T06.enc | cmp -s - T06.in; chk T06dec $?
o=$($O/T06 decode < T06bad.in 2>$O/e); rc=$?; [ -z "$o" ] && [ $rc = 1 ] && cmp -s $O/e T06bad.err; chk T06bad $?
$O/T07 1994 MCMXCIV 4 IIII 0 3999 abc MMMM XLII | cmp -s - T07.out; chk T07 $?
$O/T08 < T08.in | cmp -s - T08.out; chk T08 $?
$O/T09 < T09.in | cmp -s - T09.out; chk T09 $?
$O/T10 3 < T10.in | cmp -s - T10.out; chk T10 $?
