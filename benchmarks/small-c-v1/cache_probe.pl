#!/usr/bin/env perl
# Controlled diagnostic: same generated C/headers/flags/output, only input C path varies.
use strict;
use warnings;
use Cwd qw(abs_path);
use Digest::SHA qw(sha256_hex);
use File::Basename qw(dirname);
use File::Path qw(make_path);
use Getopt::Long qw(GetOptions);
use JSON::PP;
use POSIX qw(strftime);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC);
my ($baseline,$report);
GetOptions('baseline-json=s'=>\$baseline,'report=s'=>\$report) or die "invalid options\n";
die "need --baseline-json and --report\n" unless defined($baseline)&&defined($report)&&!@ARGV;
sub read_bytes {open my $f,'<:raw',$_[0] or die "$_[0]: $!"; local $/; return <$f>;}
sub record {my($p)=@_;return {path=>$p,sha256=>sha256_hex(read_bytes($p)),bytes=>-s $p};}
sub ensure_record {my($f)=@_;die "$f->{path} changed\n" unless sha256_hex(read_bytes($f->{path})) eq $f->{sha256};}
my $r=decode_json(read_bytes($baseline));
my ($case)=grep {$_->{id} eq 'integer_recurrence'} @{$r->{results}};
die "baseline lacks integer_recurrence\n" unless $case;
my $src=$case->{artifacts}{generated_c};
my $runtime=$r->{toolchain}{yavo_runtime};
my $zig=$r->{toolchain}{zig};
ensure_record($_) for ($src,$runtime,$zig);
my $out=dirname($src->{path}).'/cache-probe-'.time.'-'.$$;
make_path($out);
my $bytes=read_bytes($src->{path});
my $stable="$out/stable.c";
open my $f,'>:raw',$stable or die $!;print {$f}$bytes;close $f;
my $exe="$out/program";
my $json=JSON::PP->new->canonical->pretty;
my (@commands,$serial);
sub run {
    my($label,@cmd)=@_;
    my $prefix=sprintf('%s/%03d-%s',$out,++$serial,$label);
    my $start=clock_gettime(CLOCK_MONOTONIC);
    my $pid=fork();die $! unless defined $pid;
    if(!$pid){open STDIN,'<','/dev/null' or die $!;open STDOUT,'>:raw',"$prefix.stdout" or die $!;
        open STDERR,'>:raw',"$prefix.stderr" or die $!;exec 'timeout','--kill-after=2s','60s',@cmd;die $!;}
    waitpid($pid,0)==$pid or die $!;
    my $status=$?;
    my $seconds=clock_gettime(CLOCK_MONOTONIC)-$start;
    my $stdout=read_bytes("$prefix.stdout");my $stderr=read_bytes("$prefix.stderr");
    die "$label failed status=$status $stderr\n" if $status||length($stderr);
    push @commands,{label=>$label,command=>\@cmd,seconds=>$seconds,status=>$status,
                    stdout=>record("$prefix.stdout"),stderr=>record("$prefix.stderr")};
    return($stdout,$seconds);
}
sub build {
    my($label,$source)=@_;
    my(undef,$seconds)=run($label,$zig->{path},'cc',@{$r->{toolchain}{c_flags}},
        '-I',dirname($runtime->{path}),$source,'-o',$exe);
    my $artifact=record($exe);
    my($output)=run("$label-check",$exe,@{$case->{args}});
    die "$label output mismatch\n" unless $output eq $case->{expected_stdout};
    return{seconds=>$seconds,source=>record($source),artifact_sha256=>$artifact->{sha256},
           artifact_bytes=>$artifact->{bytes},output=>$output};
}
my $warmup=build('stable-prime',$stable);
my @pairs;
for my $i(0..4){
    my $fresh="$out/fresh-$i.c";
    open my $f,'>:raw',$fresh or die $!;print {$f}$bytes;close $f;
    my @order=$i%2?('fresh','stable'):('stable','fresh');
    my %pair=(order=>\@order);
    for my $variant(@order){
        $pair{$variant}=build("$variant-$i",$variant eq 'stable'?$stable:$fresh);
        die "artifact changed with source path\n" unless $pair{$variant}{artifact_sha256} eq $warmup->{artifact_sha256};
    }
    push @pairs,\%pair;
}
sub median{my @s=sort{$a<=>$b}@_;return $s[int(@s/2)];}
my $stable_m=median(map{$_->{stable}{seconds}}@pairs);
my $fresh_m=median(map{$_->{fresh}{seconds}}@pairs);
ensure_record($_) for ($src,$runtime,$zig);
my $result={schema=>'yavo-c-path-cache-probe/1',created_utc=>strftime('%Y-%m-%dT%H:%M:%SZ',gmtime),
    baseline_report=>record(abs_path($baseline)),script=>record(abs_path($0)),source=>$src,runtime=>$runtime,zig=>$zig,
    flags=>$r->{toolchain}{c_flags},output_path=>$exe,priming_build=>$warmup,pairs=>\@pairs,
    final_artifact=>record($exe),command_records=>\@commands,
    median_stable_path_seconds=>$stable_m,median_fresh_path_seconds=>$fresh_m,
    ratio_fresh_over_stable=>$fresh_m/$stable_m,all_outputs_and_binary_hashes_equal=>JSON::PP::true,
    limitations=>['Five pairs on shared Linux host; diagnostic, not a formal compile-time gate.',
        'Source bytes, runtime include directory, flags and output path stay fixed. Stable source path is primed; each fresh source path is new.',
        'Yavo currently also changes its runtime include path per invocation, so this isolates one cache input rather than all Yavo build overhead.']};
make_path(dirname($report));open my $report_file,'>:raw',"$report.tmp" or die $!;print {$report_file}$json->encode($result);close $report_file;
rename "$report.tmp",$report or die $!;
printf "stable C path %.3f ms; fresh C path %.3f ms; %.3fx\n",$stable_m*1000,$fresh_m*1000,$fresh_m/$stable_m;
