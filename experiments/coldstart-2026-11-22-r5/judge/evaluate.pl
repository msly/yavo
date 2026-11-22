#!/usr/bin/env perl
use strict;
use warnings;
use Cwd qw(abs_path);
use Digest::SHA qw(sha256_hex);
use Encode qw(encode decode FB_DEFAULT);
use File::Basename qw(dirname);
use File::Find qw(find);
use File::Path qw(make_path);
use File::Spec;
use Getopt::Long qw(GetOptions);
use JSON::PP;
use MIME::Base64 qw(encode_base64);
use POSIX qw(setsid WNOHANG strftime);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC sleep);

# A post-submission judge. This output must not be given to measured workers.
# No shell interpolation is used for compiler or submission commands.
my ($cases_path, $submission_path, $config_path, $output);
my ($build_timeout, $run_timeout, $output_limit) = (120, 10, 1048576);
GetOptions('cases=s' => \$cases_path, 'submission=s' => \$submission_path,
           'compiler-config=s' => \$config_path, 'output=s' => \$output,
           'build-timeout=f' => \$build_timeout, 'run-timeout=f' => \$run_timeout,
           'output-limit=i' => \$output_limit) or die "invalid arguments\n";
die "usage: evaluate.pl --cases FILE --submission FILE --compiler-config FILE --output DIR\n"
    unless defined($cases_path) && defined($submission_path) && defined($config_path) && defined($output);
die "limits must be positive\n" unless $build_timeout > 0 && $run_timeout > 0 && $output_limit > 0;
my $json = JSON::PP->new->canonical(1)->pretty(1)->utf8(1);

sub bytes {
    my ($path) = @_;
    open my $file, '<:raw', $path or die "open $path: $!\n";
    local $/;
    my $data = <$file>;
    close $file or die "close $path: $!\n";
    return defined($data) ? $data : '';
}
sub save_bytes {
    my ($path, $data) = @_;
    make_path(dirname($path));
    open my $file, '>:raw', $path or die "open $path: $!\n";
    print {$file} $data or die "write $path: $!\n";
    close $file or die "close $path: $!\n";
}
sub save_json {
    my ($path, $value) = @_;
    save_bytes("$path.tmp", $json->encode($value));
    rename "$path.tmp", $path or die "rename $path: $!\n";
}
sub truth { return $_[0] ? JSON::PP::true : JSON::PP::false; }
sub utc { return strftime('%Y-%m-%dT%H:%M:%SZ', gmtime()); }
sub ns { return int(clock_gettime(CLOCK_MONOTONIC) * 1000000000); }
sub relative {
    my ($base, $name) = @_;
    die "unsafe relative path\n" if !defined($name) || $name eq '' || $name =~ m{\A/|\A[A-Za-z]:|\\|(?:\A|/)\.\.(?:/|\z)|\x00};
    my $path = $base;
    for my $part (split '/', $name) {
        next if $part eq '.' || $part eq '';
        $path .= "/$part";
        die "symlink forbidden: $path\n" if -l $path;
    }
    return $path;
}
sub source_manifest {
    my ($root, $language) = @_;
    my %files;
    my $suffix = $language eq 'yavo' ? '.yv' : '.rs';
    find({no_chdir => 1, wanted => sub {
        my $path = $File::Find::name;
        die "symlink in source: $path\n" if -l $path;
        return unless -f $path && substr($path, -length($suffix)) eq $suffix;
        my $name = substr($path, length($root) + 1);
        $files{$name} = sha256_hex(bytes($path));
    }}, $root);
    return {files => \%files, sha256 => sha256_hex($json->encode(\%files))};
}
sub captured {
    my ($path) = @_;
    my $data = bytes($path);
    my $excerpt = substr($data, 0, $output_limit);
    my $display = $excerpt;
    return {path => $path, bytes => length($data), sha256 => sha256_hex($data),
            base64 => encode_base64($excerpt, ''), text_utf8 => decode('UTF-8', $display, FB_DEFAULT),
            capture_truncated => truth(length($data) > $output_limit)};
}
sub execute {
    my ($argv, $cwd, $dir, $input, $seconds, $env) = @_;
    make_path($dir);
    save_bytes("$dir/stdin", $input);
    my $start = ns();
    my $result = {argv => $argv, cwd => $cwd, environment_overrides => $env,
                  started_utc => utc(), started_monotonic_ns => $start,
                  timeout_seconds => $seconds, output_limit_bytes_per_stream => $output_limit,
                  stdin_sha256 => sha256_hex($input), status => undef, exit_code => undef};
    my $pid = fork();
    die "fork failed: $!\n" unless defined $pid;
    if (!$pid) {
        setsid() >= 0 or die "setsid: $!\n";
        open STDIN, '<:raw', "$dir/stdin" or die "stdin: $!\n";
        open STDOUT, '>:raw', "$dir/stdout" or die "stdout: $!\n";
        open STDERR, '>:raw', "$dir/stderr" or die "stderr: $!\n";
        chdir $cwd or die "chdir: $!\n";
        @ENV{keys %$env} = values %$env;
        exec { $argv->[0] } @$argv;
        die "exec $argv->[0]: $!\n";
    }
    my $wait_status;
    while (1) {
        my $waited = waitpid($pid, WNOHANG);
        if ($waited == $pid) { $wait_status = $?; last; }
        die "waitpid failed: $!\n" if $waited < 0;
        my $elapsed = (ns() - $start) / 1000000000;
        my $oversized = 0;
        for my $stream ('stdout', 'stderr') {
            my $size = -s "$dir/$stream";
            $oversized = 1 if defined($size) && $size > $output_limit;
        }
        if ($elapsed > $seconds || $oversized) {
            $result->{status} = $oversized ? 'output_limit' : 'timeout';
            kill 'KILL', -$pid;
            kill 'KILL', $pid;
            waitpid($pid, 0);
            $wait_status = $?;
            last;
        }
        sleep 0.025;
    }
    # A terminated parent must not leave a descendant running between cases.
    kill 'KILL', -$pid;
    $result->{exit_code} = ($wait_status & 127) ? 128 + ($wait_status & 127) : $wait_status >> 8;
    $result->{status} ||= 'exited';
    my $end = ns();
    $result->{finished_monotonic_ns} = $end;
    $result->{elapsed_seconds} = ($end - $start) / 1000000000;
    for my $stream ('stdout', 'stderr') {
        save_bytes("$dir/$stream", '') unless -f "$dir/$stream";
        $result->{$stream} = captured("$dir/$stream");
        $result->{status} = 'output_limit' if $result->{$stream}{capture_truncated};
    }
    save_json("$dir/command.json", $result);
    return $result;
}
sub match_case {
    my ($observed, $case) = @_;
    my %checks = (normal_exit => truth($observed->{status} eq 'exited'),
                  exit_code => truth($observed->{exit_code} == ($case->{exit_code} // 0)));
    my %expected = (exit_code => $case->{exit_code} // 0);
    for my $stream ('stdout', 'stderr') {
        my $wanted = encode('UTF-8', $case->{$stream} // '');
        $checks{$stream} = truth(!$observed->{$stream}{capture_truncated} &&
                                bytes($observed->{$stream}{path}) eq $wanted);
        $expected{"${stream}_sha256"} = sha256_hex($wanted);
    }
    return {id => $case->{id}, visibility => $case->{visibility},
            passed => truth(!(grep { !$_ } values %checks)), checks => \%checks,
            expected => \%expected, observed => $observed};
}

$cases_path = abs_path($cases_path) or die "missing cases\n";
$submission_path = abs_path($submission_path) or die "missing submission\n";
$config_path = abs_path($config_path) or die "missing compiler config\n";
make_path($output);
$output = abs_path($output) or die "missing output directory\n";
my $cases = $json->decode(bytes($cases_path));
my $submission = $json->decode(bytes($submission_path));
my $configuration = $json->decode(bytes($config_path));
my $language = $submission->{language};
die "invalid language\n" unless $language eq 'yavo' || $language eq 'rust';
my $compiler = $configuration->{$language} or die "missing compiler configuration\n";
die "compiler argv must be an array\n" unless ref($compiler->{argv}) eq 'ARRAY' && @{$compiler->{argv}};
die "identity_files is required\n" unless ref($compiler->{identity_files}) eq 'HASH' && keys %{$compiler->{identity_files}};
my %identity;
for my $path (sort keys %{$compiler->{identity_files}}) {
    my $actual = sha256_hex(bytes($path));
    die "compiler identity mismatch: $path\n" unless $actual eq $compiler->{identity_files}{$path};
    $identity{$path} = $actual;
}
my $root = dirname($submission_path);
my %inputs;
for my $task (@{$submission->{tasks}}) {
    die "duplicate submission task\n" if exists $inputs{$task->{id}};
    $inputs{$task->{id}} = $task;
}
my %known;
for my $task (@{$cases->{tasks}}) {
    die "duplicate judge task\n" if $known{$task->{id}}++;
    my %case_ids;
    for my $case (@{$task->{cases}}) {
        die "duplicate case id\n" if $case_ids{$case->{id}}++;
        die "invalid visibility\n" unless $case->{visibility} eq 'public' || $case->{visibility} eq 'hidden';
    }
}
die "unknown submission task\n" if grep { !$known{$_} } keys %inputs;

sub evaluate_stage {
    my ($task, $stage, $directory) = @_;
    return {status => 'missing_submission', build_passed => JSON::PP::false, cases => []} unless $stage;
    my $source = relative($root, $stage->{source_dir});
    my $entry = relative($source, $stage->{entry});
    return {status => 'missing_source', build_passed => JSON::PP::false, cases => []} unless -d $source && -f $entry;
    my $manifest = source_manifest($source, $language);
    make_path($directory);
    my $executable = "$directory/program";
    my @argv = @{$compiler->{argv}};
    for (@argv) {
        s/\{entry\}/$entry/g;
        s/\{output\}/$executable/g;
        s/\{source_dir\}/$source/g;
    }
    my $build = execute(\@argv, $source, "$directory/build", '', $build_timeout, $compiler->{env} // {});
    my $built = $build->{status} eq 'exited' && $build->{exit_code} == 0 && -f $executable;
    my $result = {status => $built ? 'evaluated' : 'build_failed', source => $manifest,
                  source_dir => $stage->{source_dir}, entry => $stage->{entry}, build_passed => truth($built),
                  build => $build, cases => [], historical_first_build => $stage->{historical_first_build}};
    if ($built) {
        $result->{executable_sha256} = sha256_hex(bytes($executable));
        for my $case (@{$task->{cases}}) {
            my $dir = relative("$directory/cases", $case->{id});
            my $cwd = "$dir/work";
            make_path($cwd);
            while (my ($name, $text) = each %{$case->{files} // {}}) {
                save_bytes(relative($cwd, $name), encode('UTF-8', $text));
            }
            my @command = ($executable, @{$case->{args} // []});
            my $observed = execute(\@command, $cwd, $dir, encode('UTF-8', $case->{stdin}),
                                   $run_timeout, $compiler->{run_env} // {});
            push @{$result->{cases}}, match_case($observed, $case);
        }
    }
    $result->{source_unchanged} = truth(source_manifest($source, $language)->{sha256} eq $manifest->{sha256});
    for my $visibility ('public', 'hidden') {
        my @selected = grep { $_->{visibility} eq $visibility } @{$result->{cases}};
        my $expected = grep { $_->{visibility} eq $visibility } @{$task->{cases}};
        my $passed = grep { $_->{passed} } @selected;
        $result->{$visibility} = {passed => 0 + $passed, total => 0 + $expected, executed => 0 + @selected,
                                 all_passed => truth($expected && @selected == $expected &&
                                                     $passed == $expected && $result->{source_unchanged})};
    }
    return $result;
}
my $report = {schema => 'yavo-r5-evaluation-v1', run_id => $submission->{run_id},
              language => $language, replicate => $submission->{replicate}, created_utc => utc(),
              measurement_scope => 'post_submission_replay_not_agent_cost', compiler_identities => \%identity,
              frozen_inputs => {cases_sha256 => sha256_hex(bytes($cases_path)),
                                submission_sha256 => sha256_hex(bytes($submission_path)),
                                compiler_config_sha256 => sha256_hex(bytes($config_path)),
                                judge_sha256 => sha256_hex(bytes(abs_path($0)))},
              provenance => $submission->{provenance} // {}, tasks => []};
for my $task (@{$cases->{tasks}}) {
    my $item = {id => $task->{id}, structural_requirements => $task->{structural} // []};
    for my $stage ('first', 'final') {
        eval { $item->{$stage} = evaluate_stage($task, $inputs{$task->{id}}{$stage}, "$output/$task->{id}/$stage"); 1 }
            or $item->{$stage} = {status => 'judge_error', build_passed => JSON::PP::false, error => "$@", cases => []};
    }
    push @{$report->{tasks}}, $item;
    save_json("$output/report.json", $report);
}
my $identity_unchanged = 1;
for my $path (keys %identity) {
    $identity_unchanged = 0 if sha256_hex(bytes($path)) ne $identity{$path};
}
$report->{compiler_identities_unchanged} = truth($identity_unchanged);
$report->{complete} = truth($identity_unchanged && !(grep {
    $_->{first}{status} eq 'judge_error' || $_->{final}{status} eq 'judge_error'
} @{$report->{tasks}}));
save_json("$output/report.json", $report);
print $json->encode({run_id => $report->{run_id}, complete => $report->{complete}, report => "$output/report.json"});
exit($report->{complete} ? 0 : 2);
