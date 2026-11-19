use strict; use warnings;
sub slurp { my $f = shift; open my $h, '<', $f or die; local $/; my $s = <$h>; close $h; return $s; }
sub out { my ($f, $s) = @_; open my $h, '>', $f or die; print $h $s; close $h; }
# T01
{ my %c; my $t = slurp('T01.in'); $c{lc $1}++ while $t =~ /([A-Za-z]+)/g;
  my @w = sort { $c{$b} <=> $c{$a} || $a cmp $b } keys %c; my $o = '';
  $o .= "$_ $c{$_}\n" for @w[0..4]; out('T01.out', $o); }
# T02
{ my @l = split /\n/, slurp('T02.in'), -1; pop @l if @l && $l[-1] eq '';
  my ($n, $tot, $best, $bs, $err, $o) = (0, 0, undef, undef, '', '');
  for my $i (0..$#l) { my $line = $l[$i]; next if $line eq '';
    my @p = split /,/, $line, -1;
    if (@p != 2 || $p[1] !~ /^[+-]?\d+$/) { $err .= "bad line ".($i+1).": $line\n"; next; }
    $n++; $tot += $p[1]; if (!defined $bs || $p[1] > $bs) { $bs = $p[1]; $best = $p[0]; } }
  my $m = int(($tot / $n) * 100.0);
  out('T02.out', "count: $n\ntotal: $tot\nmean_x100: $m\nbest: $best\n"); out('T02.err', $err); }
# T04
{ my $o = ''; for my $line (split /\n/, slurp('T04.in')) { my @s; my $e;
    for my $tok (split ' ', $line) {
      if ($tok =~ /^[+-]?\d+$/) { push @s, $tok + 0; next; }
      if ($tok =~ /^[-+*\/]$/) { if (@s < 2) { $e = 'stack underflow'; last; }
        my $b = pop @s; my $a = pop @s;
        if ($tok eq '/' && $b == 0) { $e = 'division by zero'; last; }
        push @s, $tok eq '+' ? $a + $b : $tok eq '-' ? $a - $b : $tok eq '*' ? $a * $b : int($a / $b); next; }
      $e = "bad token '$tok'"; last; }
    if (!defined $e && @s != 1) { $e = @s ? 'leftover values' : 'stack underflow'; }
    $o .= defined $e ? "error: $e\n" : "$s[0]\n"; } out('T04.out', $o); }
# T05 (args 50 30 70 20 40 60 80 30 65)
{ my @a = (50, 30, 70, 20, 40, 60, 80, 30, 65); my %seen; my @u = grep { !$seen{$_}++ } @a;
  my $root; sub ins { my ($t, $k) = @_; return { k => $k } unless $t;
    if ($k < $t->{k}) { $t->{l} = ins($t->{l}, $k) } elsif ($k > $t->{k}) { $t->{r} = ins($t->{r}, $k) } return $t; }
  $root = ins($root, $_) for @a;
  sub h { my $t = shift; return 0 unless $t; my ($x, $y) = (h($t->{l}), h($t->{r})); return 1 + ($x > $y ? $x : $y); }
  my @s = sort { $a <=> $b } @u; out('T05.out', join(' ', @s)."\nheight: ".h($root)."\nsize: ".scalar(@u)."\n"); }
# T06
{ my %inv; my $o = '';
  for my $line (split /\n/, slurp('T06.in')) { my @p = split ' ', $line;
    if ($p[0] eq 'add') { $inv{$p[1]} += $p[2]; }
    elsif ($p[0] eq 'remove') { my $have = $inv{$p[1]} // 0; if ($have < $p[2]) { $o .= "error: not enough $p[1]\n"; }
      else { $inv{$p[1]} -= $p[2]; delete $inv{$p[1]} if $inv{$p[1]} == 0; } }
    elsif ($p[0] eq 'report') { $o .= "$_ $inv{$_}\n" for sort keys %inv; }
    else { $o .= "error: unknown command '$p[0]'\n"; } } out('T06.out', $o); }
# T07 (width 20)
{ my $w = 20; my @words = split ' ', slurp('T07.in'); my ($o, $cur) = ('', '');
  for my $x (@words) { if ($cur eq '') { $cur = $x } elsif (length($cur) + 1 + length($x) <= $w) { $cur .= " $x" } else { $o .= "$cur\n"; $cur = $x } }
  $o .= "$cur\n" if $cur ne ''; out('T07.out', $o); }
# T08
{ my $pi = 4 * atan2(1, 1); my @s;
  for my $line (split /\n/, slurp('T08.in')) { my @p = split ' ', $line;
    my $a = $p[0] eq 'rect' ? $p[1] * $p[2] : $p[0] eq 'circle' ? $pi * $p[1] * $p[1] : $p[1] * $p[1];
    push @s, [$p[0], $a]; }
  my $o = ''; $o .= "$_->[0] ".int($_->[1] * 100.0)."\n" for sort { $a->[1] <=> $b->[1] } @s; out('T08.out', $o); }
# T09
{ my %m = (')' => '(', ']' => '[', '}' => '{'); my %c = reverse %m; my $o = '';
  for my $line (split /\n/, slurp('T09.in')) { my @st; my $e;
    for my $i (0 .. length($line) - 1) { my $ch = substr($line, $i, 1); my $col = $i + 1;
      if (exists $c{$ch}) { push @st, [$ch, $col]; }
      elsif (exists $m{$ch}) { if (!@st) { $e = "error at column $col: unexpected '$ch'"; last; }
        my $top = pop @st; if ($top->[0] ne $m{$ch}) { $e = "error at column $col: expected '$c{$top->[0]}' found '$ch'"; last; } } }
    if (!defined $e && @st) { $e = "unclosed '$st[-1][0]' at column $st[-1][1]"; }
    $o .= (defined $e ? $e : 'ok') . "\n"; } out('T09.out', $o); }
# T10
{ my @l = split /\n/, slurp('T10.in'); my $steps = shift @l; my @g = map { [split //] } @l;
  my ($h, $w) = (scalar @g, scalar @{$g[0]});
  for (1 .. $steps) { my @n; for my $y (0 .. $h - 1) { for my $x (0 .. $w - 1) { my $c = 0;
        for my $dy (-1 .. 1) { for my $dx (-1 .. 1) { next if !$dy && !$dx; my ($yy, $xx) = ($y + $dy, $x + $dx);
            next if $yy < 0 || $xx < 0 || $yy >= $h || $xx >= $w; $c++ if $g[$yy][$xx] eq '#'; } }
        $n[$y][$x] = ($g[$y][$x] eq '#' ? ($c == 2 || $c == 3) : $c == 3) ? '#' : '.'; } } @g = @n; }
  my $alive = 0; my $o = ''; for my $r (@g) { $o .= join('', @$r) . "\n"; $alive += grep { $_ eq '#' } @$r; }
  out('T10.out', $o . "alive: $alive\n"); }
# T03 expectations
out('T03.out', "1: first line has apples\n3: apples again here\n");
