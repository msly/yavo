#!/usr/bin/env perl
use strict;
use warnings;
use Cwd qw(abs_path getcwd);
use Digest::SHA qw(sha256_hex);
use Encode qw(decode FB_CROAK);
use File::Basename qw(dirname basename);
use File::Path qw(make_path);
use File::Spec;
use Getopt::Long qw(GetOptions);
use JSON::PP;
use POSIX qw(WNOHANG setsid);
use Time::HiRes qw(time sleep);

# Run from the repository root. No production build flag or lockfile is changed.
my ($out, @cases) = ('compiler/target/independent-sanitizers/bundle.json');
GetOptions('out=s' => \$out, 'case=s@' => \@cases) or die "usage: perl tools/sanitizers/export.pl [--out bundle.json] [--case entry.yv ...]\n";
my $root = abs_path('.');
die "run from the Yavo repository root\n" unless -f 'compiler/Cargo.toml';
$out = File::Spec->rel2abs($out, $root);
make_path(dirname($out));
my $stage = "$out.work";
die "output stage already exists: $stage; choose a fresh --out\n" if -e $stage;
make_path($stage);

sub read_bytes {
    my ($p) = @_;
    open my $f, '<:raw', $p or die "$p: $!\n";
    local $/;
    my $bytes = <$f>;
    return defined($bytes) ? $bytes : '';
}
sub file_record {
    my ($p) = @_;
    my $b = read_bytes($p);
    return { path => $p, bytes => length($b), sha256 => sha256_hex($b) };
}
sub capture {
    my ($name, $cwd, $limit, @argv) = @_;
    my $prefix = "$stage/$name";
    my $started = time;
    my $pid = fork();
    die "fork: $!" unless defined $pid;
    if (!$pid) {
        setsid() >= 0 or die "setsid: $!";
        chdir $cwd or die "chdir: $!";
        open STDIN, '<', '/dev/null' or die $!;
        open STDOUT, '>:raw', "$prefix.stdout" or die $!;
        open STDERR, '>:raw', "$prefix.stderr" or die $!;
        delete @ENV{qw(YAVO_CC LD_PRELOAD LD_LIBRARY_PATH ASAN_OPTIONS UBSAN_OPTIONS TSAN_OPTIONS)};
        exec { $argv[0] } @argv or POSIX::_exit(127);
    }
    my ($timed_out, $status) = (0, undef);
    while (1) {
        my $done = waitpid($pid, WNOHANG);
        if ($done == $pid) { $status = $?; last; }
        die "waitpid: $!" if $done < 0;
        if (time - $started > $limit) {
            $timed_out = 1;
            kill 'TERM', -$pid;
            sleep 0.2;
            kill 'KILL', -$pid;
            waitpid($pid, 0);
            $status = $?;
            last;
        }
        sleep 0.02;
    }
    my $r = { argv => \@argv, cwd => $cwd, timeout_seconds => $limit,
        timed_out => $timed_out ? JSON::PP::true : JSON::PP::false,
        exit_code => ($status & 127) ? undef : $status >> 8,
        signal => $status & 127, elapsed_seconds => time - $started,
        stdout => file_record("$prefix.stdout"), stderr => file_record("$prefix.stderr") };
    die "$name failed; inspect $prefix.stdout and .stderr\n"
        if $timed_out || $status != 0;
    return $r;
}
sub snapshot {
    open my $g, '-|', 'git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard' or die $!;
    local $/;
    my @names = split /\0/, <$g>;
    close $g or die "git ls-files failed\n";
    my %seen;
    return [ map { file_record($_) }
        sort grep { !$seen{$_}++ && -f $_ &&
            (/^compiler\/(?:src\/.*\.rs|runtime\/.*\.h|Cargo\.(?:toml|lock)|toolchain\.lock)$/ || /^toolchain\.lock$/ ||
             /\.yv$/ || /^tools\/sanitizers\//) } @names ];
}
my $before = snapshot();
my $build = capture('cargo-build', "$root/compiler", 900, 'cargo', 'build', '--locked', '--offline', '--release');
my $compiler = "$root/compiler/target/release/yavo";
my $compiler_info = file_record($compiler);
my $version_run = capture('yavo-version', $root, 10, $compiler, '--version');
$compiler_info->{version} = read_bytes($version_run->{stdout}{path});
my $header = read_bytes('compiler/runtime/yavo_rt.h');
my $expectations = decode_json(read_bytes('tools/sanitizers/expectations.json'));
my @entries = @cases ? @cases : (glob('compiler/tests/p0/run_*.yv'), glob('compiler/tests/p0/panic_*.yv'));
die "no runnable fixtures selected\n" unless @entries;
my @programs;
my %ids;
for my $entry (sort @entries) {
    my $path = abs_path($entry) // die "missing $entry\n";
    die "entry outside repository: $path\n" unless index($path, "$root/") == 0;
    my $relative = substr($path, length($root) + 1);
    my $id = $relative;
    $id =~ s/[^A-Za-z0-9_.-]+/_/g;
    die "duplicate fixture ID $id\n" if $ids{$id}++;
    my $src = read_bytes($path);
    my ($stdout, $in_stdout, $has_stdout, $panic) = ('', 0, 0, undef);
    for my $line (split /\n/, $src) {
        $line =~ s/\r$//;
        last unless $line =~ s{^// ?}{};
        if ($line eq 'expect-stdout:') { $in_stdout = 1; $has_stdout = 1; next; }
        if ($in_stdout && $line =~ s/^\| ?//) { $stdout .= "$line\n"; next; }
        $in_stdout = 0;
        die "error fixture selected: $relative\n" if $line =~ /^expect-error:/;
        if ($line =~ /^expect-panic:\s*(\S+)(?:\s+(\d+):(\d+))?\s*$/) {
            $panic = { category => $1, line => defined $2 ? 0+$2 : undef, column => defined $3 ? 0+$3 : undef };
        }
    }
    die "fixture needs exact expect-stdout or expect-panic: $relative\n" unless $has_stdout || $panic;
    my $c_path = "$stage/$id.c";
    my $front = capture("frontend-$id", $root, 60, $compiler, 'check', $path, '--emit-c', $c_path, '--json');
    my $diagnostics = decode_json(read_bytes($front->{stdout}{path}));
    die "frontend reported errors for $relative\n" if ($diagnostics->{error_count} // 0) != 0;
    my $c = read_bytes($c_path);
    my $source_hash = sha256_hex($src);
    my $expected = { stdout => $has_stdout ? decode('UTF-8', $stdout, FB_CROAK) : '', stderr => '',
        exit_code => $panic ? 101 : 0, panic => $panic };
    if (my $override = $expectations->{$relative}) {
        die "stale explicit expectation for $relative\n" unless $override->{source_sha256} eq $source_hash;
        $expected->{$_} = $override->{$_} for grep { exists $override->{$_} } qw(stdout stderr);
    }
    push @programs, { id => $id, entry => $relative, source_sha256 => $source_hash,
        source => decode('UTF-8', $src, FB_CROAK), c_sha256 => sha256_hex($c),
        c => decode('UTF-8', $c, FB_CROAK), frontend => $front,
        expected => $expected };
}
my $after = snapshot();
my $json = JSON::PP->new->utf8->canonical;
die "repository inputs changed during export; discard bundle and export again\n"
    unless $json->encode($before) eq $json->encode($after);
my $head = capture('git-head', $root, 10, 'git', 'rev-parse', 'HEAD');
my $status = capture('git-status', $root, 10, 'git', 'status', '--porcelain');
my $bundle = { schema => 'yavo-independent-sanitizer-bundle/1', repository => $root,
    exported_epoch => time, source_revision => read_bytes($head->{stdout}{path}),
    git_status => read_bytes($status->{stdout}{path}), compiler => $compiler_info,
    compiler_build => $build, repository_inputs => $after, programs => \@programs,
    runtime => { name => 'yavo_rt.h', sha256 => sha256_hex($header), content => decode('UTF-8', $header, FB_CROAK) },
    scope => 'Generated C validation on an independent host; does not alter pinned-toolchain conformance results.' };
open my $f, '>:raw', $out or die "$out: $!";
print {$f} $json->encode($bundle), "\n";
close $f or die $!;
print $json->pretty->encode({bundle=>file_record($out), programs=>scalar(@programs), compiler=>$compiler_info});
