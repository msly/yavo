#!/usr/bin/perl
# Reference implementations for TASKS.md. Usage: perl ref.pl TNN [args] < input
use strict; use warnings;
my $t = shift @ARGV;
my $num = qr/^-?\d+(\.\d+)?$/;
sub lines { my @l = <STDIN>; chomp @l; return @l }

if ($t eq 'T01') {
    my @l = lines(); my @h = split /,/, shift(@l), -1;
    my @cols = map { [] } @h;
    for (@l) { my @v = split /,/, $_, -1; push @{$cols[$_]}, $v[$_] for 0..$#h }
    for my $i (0..$#h) {
        my @v = grep { $_ ne '' } @{$cols[$i]};
        if (!grep { $_ !~ $num } @v) {
            my ($mn, $mx, $s) = ($v[0], $v[0], 0);
            for (@v) { $mn = $_ if $_ < $mn; $mx = $_ if $_ > $mx; $s += $_ }
            printf "%s: min=%.2f max=%.2f mean=%.2f\n", $h[$i], $mn, $mx, $s / @v;
        } else { my %d = map { $_ => 1 } @v; printf "%s: text (%d distinct)\n", $h[$i], scalar keys %d }
    }
} elsif ($t eq 'T02') {
    my (%c, %h, $first, $skip); $skip = 0;
    for (lines()) {
        my @f = split ' ', $_;
        if (@f < 2) { $skip++; next }
        $c{$f[1]}++; $h{substr($f[0], 0, 2)}++;
        $first //= join(' ', @f[2..$#f]) if $f[1] eq 'ERROR';
    }
    print "$_ $c{$_}\n" for sort { $c{$b} <=> $c{$a} || $a cmp $b } keys %c;
    print "first error: ", ($first // 'none'), "\n";
    my ($bh) = sort { $h{$b} <=> $h{$a} || $a cmp $b } keys %h;
    print "busiest hour: $bh ($h{$bh})\n"; print "skipped: $skip\n";
} elsif ($t eq 'T03') {
    my %b;
    for (lines()) {
        my @f = split ' ', $_; next unless @f;
        my %ar = (open => 2, deposit => 3, withdraw => 3, transfer => 4);
        if (!exists $ar{$f[0]} || @f != $ar{$f[0]}) { print "error: bad command\n"; next }
        if ($f[0] eq 'open') { if (exists $b{$f[1]}) { print "error: $f[1] exists\n" } else { $b{$f[1]} = 0 } next }
        my $amt = $f[-1];
        if ($amt !~ /^\d+$/ || $amt == 0) { print "error: bad amount '$amt'\n"; next }
        my @acc = $f[0] eq 'transfer' ? @f[1,2] : ($f[1]);
        my ($miss) = grep { !exists $b{$_} } @acc;
        if (defined $miss) { print "error: no account $miss\n"; next }
        if ($f[0] eq 'transfer' && $f[1] eq $f[2]) { print "error: same account\n"; next }
        if ($f[0] eq 'deposit') { $b{$f[1]} += $amt; next }
        if ($b{$f[1]} < $amt) { print "error: insufficient funds in $f[1]\n"; next }
        $b{$f[1]} -= $amt; $b{$f[2]} += $amt if $f[0] eq 'transfer';
    }
    my $tot = 0; for (sort keys %b) { print "$_ $b{$_}\n"; $tot += $b{$_} } print "total: $tot\n";
} elsif ($t eq 'T04') {
    my (@m, $k) = ([], []); $k = 0;
    for (lines()) { if ($_ eq '') { $k = 1; next } push @{$m[$k]}, [split ' ', $_] }
    for my $x (@m) { for (@$x) { if (@$_ != @{$x->[0]}) { print STDERR "error: ragged matrix\n"; exit 1 } } }
    my ($a, $b) = @m; my ($ar, $ac, $br, $bc) = (scalar @$a, scalar @{$a->[0]}, scalar @$b, scalar @{$b->[0]});
    if ($ac != $br) { print STDERR "error: cannot multiply ${ar}x${ac} by ${br}x${bc}\n"; exit 1 }
    for my $i (0..$ar-1) { print join(' ', map { my $j = $_; my $s = 0; $s += $a->[$i][$_] * $b->[$_][$j] for 0..$ac-1; $s } 0..$bc-1), "\n" }
} elsif ($t eq 'T05') {
    my %d;
    for (lines()) { my ($n, $r) = split /:/, $_, 2; $d{$n} //= {}; for (split ' ', $r) { $d{$n}{$_} = 1; $d{$_} //= {} } }
    my %done;
    while (1) {
        my ($n) = sort grep { !$done{$_} && !grep { !$done{$_} } keys %{$d{$_}} } keys %d;
        last unless defined $n; print "$n\n"; $done{$n} = 1;
    }
    my @rest = sort grep { !$done{$_} } keys %d;
    if (@rest) { print "cycle: @rest\n"; exit 1 }
} elsif ($t eq 'T06') {
    local $/; my $in = <STDIN>;
    if ($ARGV[0] eq 'encode') { my $o = ''; while ($in =~ /((.)\2*)/sg) { $o .= length($1) . $2 } print $o; exit 0 }
    my ($o, $i, $n) = ('', 0, length $in);
    while ($i < $n) {
        my $s = $i; $i++ while $i < $n && substr($in, $i, 1) =~ /\d/;
        if ($i == $s) { print STDERR "invalid input at byte $s\n"; exit 1 }
        my $c = substr($in, $s, $i - $s);
        if ($c == 0 || $i >= $n) { print STDERR "invalid input at byte $s\n"; exit 1 }
        $o .= substr($in, $i, 1) x $c; $i++;
    }
    print $o;
} elsif ($t eq 'T07') {
    my @r = ([1000,'M'],[900,'CM'],[500,'D'],[400,'CD'],[100,'C'],[90,'XC'],[50,'L'],[40,'XL'],[10,'X'],[9,'IX'],[5,'V'],[4,'IV'],[1,'I']);
    my $to = sub { my $n = shift; my $s = ''; for (@r) { while ($n >= $_->[0]) { $s .= $_->[1]; $n -= $_->[0] } } $s };
    my %v = (I=>1,V=>5,X=>10,L=>50,C=>100,D=>500,M=>1000);
    for my $a (@ARGV) {
        if ($a =~ /^\d+$/) { print $a >= 1 && $a <= 3999 ? "$a -> " . $to->($a) . "\n" : "$a: invalid\n"; next }
        if ($a !~ /^[IVXLCDM]+$/) { print "$a: invalid\n"; next }
        my @c = map { $v{$_} } split //, $a; my $n = 0;
        for my $i (0..$#c) { $n += ($i < $#c && $c[$i] < $c[$i+1]) ? -$c[$i] : $c[$i] }
        print $n >= 1 && $n <= 3999 && $to->($n) eq $a ? "$a -> $n\n" : "$a: invalid\n";
    }
} elsif ($t eq 'T08') {
    my %ar = (circle => 1, rect => 2, triangle => 3); my ($tot, $best, $bk, $ln) = (0, -1, 'none', 0);
    for (lines()) {
        $ln++; my @f = split ' ', $_; my $k = shift @f // '';
        if (!exists $ar{$k} || @f != $ar{$k} || grep { $_ !~ $num } @f) { print "line $ln: cannot parse\n"; next }
        my $area;
        if ($k eq 'circle') { $area = 4 * atan2(1, 1) * $f[0] * $f[0] }
        elsif ($k eq 'rect') { $area = $f[0] * $f[1] }
        else {
            my ($x, $y, $z) = @f;
            if ($x + $y <= $z || $x + $z <= $y || $y + $z <= $x) { print "triangle: invalid\n"; next }
            my $s = ($x + $y + $z) / 2; $area = sqrt($s * ($s - $x) * ($s - $y) * ($s - $z));
        }
        printf "%s: %.2f\n", $k, $area; $tot += $area;
        if ($area > $best) { $best = $area; $bk = $k }
    }
    printf "total: %.2f\n", $tot; print "largest: $bk\n";
} elsif ($t eq 'T09') {
    my %root;
    for (lines()) {
        my ($p, $s) = split ' ', $_; my @parts = split m{/}, $p; my $file = pop @parts; my $d = \%root;
        for (@parts) { $d->{$_} //= {}; $d = $d->{$_} }
        $d->{$file} += $s;
    }
    my $sum; $sum = sub { my $d = shift; my $t = 0; for (values %$d) { $t += ref $_ ? $sum->($_) : $_ } $t };
    print "/ ", $sum->(\%root), "\n";
    my $walk; $walk = sub { my ($d, $ind) = @_;
        for (sort keys %$d) { my $v = $d->{$_};
            if (ref $v) { print "$ind$_/ ", $sum->($v), "\n"; $walk->($v, "$ind  ") } else { print "$ind$_ $v\n" } } };
    $walk->(\%root, '  ');
} elsif ($t eq 'T10') {
    my $cap = $ARGV[0]; my (@o, %v, $h, $m); ($h, $m) = (0, 0);
    my $touch = sub { my $k = shift; @o = ($k, grep { $_ ne $k } @o) };
    for (lines()) {
        my @f = split ' ', $_;
        if ($f[0] eq 'get') { if (exists $v{$f[1]}) { $h++; print "$v{$f[1]}\n"; $touch->($f[1]) } else { $m++; print "miss\n" } }
        else { my $new = !exists $v{$f[1]}; $v{$f[1]} = $f[2]; $touch->($f[1]);
            if ($new && @o > $cap) { my $e = pop @o; delete $v{$e}; print "evict $e\n" } }
    }
    print "hits: $h misses: $m\n"; print "order: @o\n";
}
