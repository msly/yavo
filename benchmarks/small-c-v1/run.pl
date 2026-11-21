#!/usr/bin/env perl
use strict;
use warnings;
use Cwd qw(abs_path);
use Digest::SHA qw(sha256_hex);
use File::Basename qw(dirname);
use File::Find;
use File::Path qw(make_path);
use FindBin;
use Getopt::Long qw(GetOptions);
use JSON::PP;
use POSIX qw(strftime);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC);

my $suite = abs_path($FindBin::Bin);
my $repo = abs_path("$suite/../..");
my ($compiler_repo, $out, $report_path, $cpu) = ($repo, undef, undef, undef);
GetOptions('compiler-repo=s' => \$compiler_repo, 'out=s' => \$out,
           'report=s' => \$report_path, 'cpu=i' => \$cpu) or die "invalid options\n";
die "unexpected arguments\n" if @ARGV;
$compiler_repo = abs_path($compiler_repo) // die "compiler repository missing\n";
die "YAVO_CC/YAVO_ALLOC_CHECK must be unset\n" if exists $ENV{YAVO_CC} || exists $ENV{YAVO_ALLOC_CHECK};
my $stamp = strftime('%Y%m%dT%H%M%SZ', gmtime);
$out //= "$repo/compiler/target/small-c-v1/$stamp-$$";
die "output directory exists: $out\n" if -e $out;
make_path($out);
$out = abs_path($out);
make_path("$out/logs");
$report_path //= "$out/report.json";
$report_path = "$repo/$report_path" unless $report_path =~ m{^/};
make_path(dirname($report_path));
my $json = JSON::PP->new->canonical->pretty;
my (@commands, $serial);

sub read_file {
    my ($path) = @_;
    open my $fh, '<:raw', $path or die "$path: $!\n";
    local $/;
    return <$fh>;
}
sub sha { return sha256_hex(read_file($_[0])); }
sub file_record {
    my ($path) = @_;
    return {path=>$path, bytes=>-s $path, sha256=>sha($path)};
}
sub run_command {
    my ($label, $deadline, @cmd) = @_;
    my $number = ++$serial;
    my $log = sprintf('%s/logs/%04d-%s', $out, $number, $label);
    my $start = clock_gettime(CLOCK_MONOTONIC);
    my $pid = fork();
    die "fork: $!\n" unless defined $pid;
    if (!$pid) {
        open STDIN, '<', '/dev/null' or die $!;
        open STDOUT, '>:raw', "$log.stdout" or die $!;
        open STDERR, '>:raw', "$log.stderr" or die $!;
        exec 'timeout', '--signal=TERM', '--kill-after=2s', $deadline.'s', @cmd;
        die "exec: $!\n";
    }
    waitpid($pid, 0) == $pid or die "waitpid: $!\n";
    my $status = $?;
    my $elapsed = clock_gettime(CLOCK_MONOTONIC) - $start;
    my $stdout = read_file("$log.stdout");
    my $stderr = read_file("$log.stderr");
    my $entry = {label=>$label, command=>\@cmd, elapsed_seconds=>$elapsed,
                 exit_code=>($status >> 8), signal=>($status & 127),
                 stdout=>file_record("$log.stdout"), stderr=>file_record("$log.stderr")};
    push @commands, $entry;
    die "$label failed (status=$status):\n$stdout\n$stderr\n" if $status;
    return ($stdout, $stderr, $entry);
}
sub clean_command {
    my ($label, $deadline, @cmd) = @_;
    my ($stdout, $stderr, $entry) = run_command($label, $deadline, @cmd);
    die "$label emitted diagnostics:\n$stderr\n" if length $stderr;
    return ($stdout, $entry);
}
sub compiler_sources {
    my @files;
    for my $dir ("$compiler_repo/compiler/src", "$compiler_repo/compiler/runtime") {
        find({no_chdir=>1, wanted=>sub { push @files, $File::Find::name if -f $_; }}, $dir);
    }
    push @files, map {"$compiler_repo/compiler/$_"} qw(Cargo.toml Cargo.lock toolchain.lock);
    return [map {file_record($_)} sort @files];
}
sub assert_same_sources {
    my ($before, $after, $label) = @_;
    die "$label changed while measurement ran\n" unless $json->encode($before) eq $json->encode($after);
}
sub distribution {
    my (@values) = @_;
    my @s = sort {$a <=> $b} @values;
    my $n = @s;
    my $median = $n % 2 ? $s[int($n/2)] : ($s[$n/2-1]+$s[$n/2])/2;
    my $p95 = int(0.95*$n); $p95-- if $p95 == 0.95*$n;
    return {samples_seconds=>\@values, n=>$n, median_seconds=>$median,
            p95_nearest_rank_seconds=>$s[$p95], min_seconds=>$s[0], max_seconds=>$s[-1]};
}
sub elf_record {
    my ($path) = @_;
    my $b = read_file($path);
    die "$path not x86_64 little-endian ELF64\n" unless substr($b,0,6) eq "\x7fELF\x02\x01"
        && unpack('v',substr($b,18,2)) == 62;
    my ($phoff,$shoff) = (unpack('Q<',substr($b,32,8)),unpack('Q<',substr($b,40,8)));
    my ($phsize,$phnum,$shsize,$shnum) = unpack('vvvv',substr($b,54,8));
    for my $i (0..$phnum-1) {
        my $type = unpack('V',substr($b,$phoff+$i*$phsize,4));
        die "$path is dynamically linked\n" if $type == 2 || $type == 3;
    }
    for my $i (0..$shnum-1) {
        die "$path retains a full symbol table\n" if unpack('V',substr($b,$shoff+$i*$shsize+4,4)) == 2;
    }
    return {%{file_record($path)}, format=>'ELF64', architecture=>'x86_64',
            static_link_verified=>JSON::PP::true, full_symbol_table_absent=>JSON::PP::true};
}
my $manifest = decode_json(read_file("$suite/manifest.json"));
die "unsupported schema\n" unless $manifest->{schema} eq 'yavo-small-c-bench/1';
die "expected seven measurements\n" unless $manifest->{samples} == 7;
my @suite_paths = ("$suite/manifest.json","$suite/run.pl","$suite/reference.rs","$suite/common.h");
push @suite_paths, map {("$suite/$_->{id}.yv","$suite/$_->{id}.c")} @{$manifest->{cases}};
my $suite_sources = [map {file_record($_)} sort @suite_paths];
my $sources_before = compiler_sources();
my ($commit) = clean_command('compiler-commit', 10, 'git','-C',$compiler_repo,'rev-parse','HEAD');
chomp $commit;
my ($dirty) = clean_command('compiler-status', 10, 'git','-C',$compiler_repo,'status','--porcelain',
    '--','compiler/src','compiler/runtime','compiler/Cargo.toml','compiler/Cargo.lock','compiler/toolchain.lock');
my $lock = read_file("$compiler_repo/compiler/toolchain.lock");
$lock =~ /^zig_version\s*=\s*(\S+)/m or die "missing pinned Zig version\n";
my $zig_version = $1;
my $zig = "$ENV{HOME}/.yavo/toolchains/zig-x86_64-linux-$zig_version/zig";
my $rustc = "$ENV{HOME}/.cargo/bin/rustc";
my $cargo = "$ENV{HOME}/.cargo/bin/cargo";
my $linker = "$ENV{HOME}/.yavo/bin/rust-linker";
die "toolchain missing\n" unless -x $zig && -x $rustc && -x $cargo && -x $linker;
my ($zig_actual) = clean_command('zig-version', 10, $zig, 'version');
chomp $zig_actual;
die "Zig version mismatch\n" unless $zig_actual eq $zig_version;
my ($rust_version) = clean_command('rust-version', 10, $rustc, '--version');
my ($cargo_version) = clean_command('cargo-version', 10, $cargo, '--version');
my ($sysroot) = clean_command('rust-sysroot', 10, $rustc, '--print','sysroot');
chomp $sysroot;
my $proc_status = read_file('/proc/self/status');
$proc_status =~ /^Cpus_allowed_list:\s*(.+)$/m or die "missing CPU affinity\n";
my $allowed_list = $1;
my %allowed;
for my $piece (split /,/, $allowed_list) {
    if ($piece =~ /^(\d+)-(\d+)$/) { $allowed{$_}=1 for $1..$2; }
    elsif ($piece =~ /^(\d+)$/) { $allowed{$1}=1; }
    else { die "invalid affinity range\n"; }
}
$cpu //= (sort {$a<=>$b} keys %allowed)[-1];
die "requested CPU not allowed\n" unless $allowed{$cpu};
my ($uname) = clean_command('uname', 10, 'uname','-a');
my $cpuinfo = read_file('/proc/cpuinfo');
$cpuinfo =~ /^model name\s*:\s*(.+)$/m;
my $cpu_model = $1 // 'unknown';
my $load_before = read_file('/proc/loadavg');
local $ENV{CARGO_TARGET_DIR} = "$compiler_repo/compiler/target";
local $ENV{CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER} = $linker;
local $ENV{PATH} = "$ENV{HOME}/.cargo/bin:$ENV{PATH}";
run_command('build-compiler', 240, $cargo,'build','--locked','--offline','--release',
    '--manifest-path',"$compiler_repo/compiler/Cargo.toml");
assert_same_sources($sources_before, compiler_sources(), 'compiler inputs');
my $yavo = "$compiler_repo/compiler/target/release/yavo";
my $compiler_binary = file_record($yavo);
my $oracle = "$out/reference";
clean_command('build-oracle', 120, $rustc,'--edition=2021','-O','-C',"linker=$linker",
    "$suite/reference.rs",'-o',$oracle);
my @results;
my @cflags = ('-std=c17','-fno-strict-aliasing','-target',$manifest->{target},'-O2','-s');
for my $case (@{$manifest->{cases}}) {
    my $id = $case->{id};
    my $dir = "$out/$id";
    make_path($dir);
    my ($yexe,$cexe,$generated) = ("$dir/yavo","$dir/c","$dir/generated.c");
    my @yb = ($yavo,'build',"$suite/$id.yv",'-o',$yexe,'--release','--target',$manifest->{target},
              '--emit-c',$generated);
    my @cb = ($zig,'cc',@cflags,'-I',$suite,"$suite/$id.c",'-o',$cexe);
    my (undef,$yfirst) = clean_command("$id-yavo-first-build",180,@yb);
    my (undef,$cfirst) = clean_command("$id-c-first-build",180,@cb);
    my (@yw,@cw);
    for my $i (0..$manifest->{warm_build_samples}-1) {
        for my $language ($i%2 ? ('c','yavo') : ('yavo','c')) {
            my (undef,$entry) = clean_command("$id-$language-warm-build-$i",180,
                                              $language eq 'yavo' ? @yb : @cb);
            push @{$language eq 'yavo' ? \@yw : \@cw},$entry->{elapsed_seconds};
        }
    }
    my @small;
    for my $i (0..$#{$case->{small_args}}) {
        my @args = @{$case->{small_args}[$i]};
        my ($expected) = clean_command("$id-reference-small-$i",30,$oracle,$id,@args);
        for my $language ('yavo','c') {
            my ($actual) = clean_command("$id-$language-small-$i",30,
                $language eq 'yavo' ? $yexe : $cexe,@args);
            die "$id $language small result mismatch\n" unless $actual eq $expected;
        }
        push @small,{args=>\@args,expected_stdout=>$expected,passed=>JSON::PP::true};
    }
    my @args = @{$case->{args}};
    my ($expected) = clean_command("$id-reference-full",60,$oracle,$id,@args);
    my (@ys,@cs,@warmups,@pairs);
    for my $i (0..$manifest->{warmup_runs}+$manifest->{samples}-1) {
        my $warm = $i < $manifest->{warmup_runs};
        my @order = $i%2 ? ('c','yavo') : ('yavo','c');
        my %pair = (order=>\@order);
        for my $language (@order) {
            my ($actual,$entry) = clean_command("$id-$language-run-$i",60,'taskset','-c',$cpu,
                $language eq 'yavo' ? $yexe : $cexe,@args);
            die "$id $language full result mismatch\n" unless $actual eq $expected;
            $pair{$language}=$entry->{elapsed_seconds};
            push @{$language eq 'yavo' ? \@ys : \@cs}, $entry->{elapsed_seconds} unless $warm;
        }
        push @{$warm ? \@warmups : \@pairs},\%pair;
    }
    my $ys = distribution(@ys);
    my $cs = distribution(@cs);
    my $ratio = $ys->{median_seconds}/$cs->{median_seconds};
    push @results,{id=>$id,description=>$case->{description},args=>\@args,small_correctness=>\@small,
        expected_stdout=>$expected,all_outputs_match=>JSON::PP::true,
        yavo=>$ys,c=>$cs,ratio_yavo_over_c=>$ratio,warmup_pairs=>\@warmups,measured_pairs=>\@pairs,
        builds=>{first_observed_yavo_seconds=>$yfirst->{elapsed_seconds},
                  first_observed_c_seconds=>$cfirst->{elapsed_seconds},
                  warm_yavo=>distribution(@yw),warm_c=>distribution(@cw)},
        artifacts=>{yavo=>elf_record($yexe),c=>elf_record($cexe),generated_c=>file_record($generated)}};
    printf "%s: Yavo %.6fs / C %.6fs = %.3fx; outputs match\n",$id,
        $ys->{median_seconds},$cs->{median_seconds},$ratio;
}
assert_same_sources($sources_before,compiler_sources(),'compiler inputs');
assert_same_sources($suite_sources,[map {file_record($_)} sort @suite_paths],'benchmark sources');
die "compiler binary changed during measurement\n" unless sha($yavo) eq $compiler_binary->{sha256};
my $sumlog=0;
$sumlog += log($_->{ratio_yavo_over_c}) for @results;
my ($worst) = sort {$b->{ratio_yavo_over_c}<=>$a->{ratio_yavo_over_c}} @results;
my $report = {
    schema=>'yavo-small-c-performance/1',created_utc=>$stamp,
    compiler=>{repository=>$compiler_repo,head=>$commit,source_status=>$dirty,sources=>$sources_before,
               executable=>$compiler_binary,release_build_invoked=>JSON::PP::true},
    benchmark=>{manifest=>$manifest,sources=>$suite_sources,source_hashes_unchanged=>JSON::PP::true},
    toolchain=>{zig=>{%{file_record($zig)},version=>$zig_actual},lock=>file_record("$compiler_repo/compiler/toolchain.lock"),
                rustc=>{%{file_record("$sysroot/bin/rustc")},version=>$rust_version},
                cargo_version=>$cargo_version,reference_binary=>file_record($oracle),c_flags=>\@cflags,
                allocator=>'musl malloc/realloc/free for both measured languages',linkage=>'static musl',
                yavo_runtime=>file_record("$compiler_repo/compiler/runtime/yavo_rt.h"),
                yavo_safety_checks=>'on; no disabling flags or runtime patches',
                environment=>{RUSTFLAGS=>$ENV{RUSTFLAGS},CARGO_ENCODED_RUSTFLAGS=>$ENV{CARGO_ENCODED_RUSTFLAGS},
                              ZIG_GLOBAL_CACHE_DIR=>$ENV{ZIG_GLOBAL_CACHE_DIR},ZIG_LOCAL_CACHE_DIR=>$ENV{ZIG_LOCAL_CACHE_DIR}}},
    host=>{uname=>$uname,cpu_model=>$cpu_model,cpu_affinity=>$cpu,allowed_cpu_list=>$allowed_list,
           loadavg_before=>$load_before,loadavg_after=>read_file('/proc/loadavg')},
    timing=>{clock=>'CLOCK_MONOTONIC',scope=>'end-to-end child process including startup, argv parsing, final checksum output and teardown',
             warmup_runs_per_language=>$manifest->{warmup_runs},measured_runs_per_language=>$manifest->{samples},
             order=>'alternating C/Yavo within each pair; fixed case order frozen in manifest',
             affinity=>'taskset pins measured programs; host is shared and CPU frequency/governor are not controlled',
             compile_scope=>'first observed builds use existing caches; five subsequent warm release builds per language',
             cold_builds_measured=>JSON::PP::false,debug_builds_measured=>JSON::PP::false},
    results=>\@results,
    summary=>{geometric_mean_ratio=>exp($sumlog/@results),worst_case=>$worst->{id},
              worst_ratio=>$worst->{ratio_yavo_over_c},formal_v1_gate_claimed=>JSON::PP::false},
    limitations=>[
        'Four deliberately small microbenchmarks, frozen before timing; not the formal pre-registered v1 benchmark set.',
        'Only x86_64 Linux; no cross-platform runtime performance conclusion.',
        'C hot arithmetic/indexing is unchecked on inputs proven within int64/index bounds; Yavo safety checks remain on.',
        'Runtime is end-to-end; startup and argument parsing differ, and the shared VM may add scheduling noise.',
        'Both containers store contiguous int64 data with matching Vec/Deque descriptor layouts and growth policy; general allocators, strings, maps, threads, FFI and Arena are not measured.',
        'Only warm release program builds are sampled; this is not a complete cold/warm debug/release compile-latency acceptance result.',
        'Seven timing samples and five build samples are descriptive; no statistical superiority or regression gate is asserted.'
    ],
    command_records=>\@commands
};
open my $rf,'>:raw',"$report_path.tmp" or die $!;
print {$rf} $json->encode($report);
close $rf or die $!;
rename "$report_path.tmp",$report_path or die $!;
printf "geomean %.3fx; worst %s %.3fx\n",$report->{summary}{geometric_mean_ratio},$worst->{id},$worst->{ratio_yavo_over_c};
print "report: $report_path\n";
