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
use FindBin;
use JSON::PP;
use MIME::Base64 qw(encode_base64);
use POSIX qw(setsid WNOHANG strftime);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC sleep);

# A post-submission judge. This output must not be given to measured workers.
# No shell interpolation is used for compiler or submission commands.
my ($cases_path, $submission_path, $config_path, $output);
my $experiment_root = "$FindBin::Bin/..";
my $qualification_path;
my ($build_timeout, $run_timeout, $output_limit) = (120, 10, 1048576);
GetOptions('cases=s' => \$cases_path, 'submission=s' => \$submission_path,
           'compiler-config=s' => \$config_path, 'output=s' => \$output,
           'experiment-root=s' => \$experiment_root, 'qualification=s' => \$qualification_path,
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
    return {files => \%files, sha256 => sha256_hex(JSON::PP->new->canonical(1)->utf8(1)->encode(\%files))};
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

sub sealed_cohort {
    my $base = abs_path($experiment_root) or die "missing experiment root\n";
    my $freeze_path = "$base/evidence/frozen.json";
    die "hidden judging blocked: runtime freeze missing\n" unless -f $freeze_path;
    my $freeze = $json->decode(bytes($freeze_path));
    my $freeze_sha = sha256_hex(bytes($freeze_path));
    die "invalid R6 experiment freeze\n" unless ($freeze->{kind} // '') eq 'yavo.docs_ab.freeze'
        && ($freeze->{experiment_id} // '') eq 'docs-ab-2026-11-22-r6';
    my $limits = $freeze->{judge_limits} // {build_timeout => 120, run_timeout => 10, output_limit => 1048576};
    die "judge limits differ from frozen policy\n" unless $build_timeout == $limits->{build_timeout}
        && $run_timeout == $limits->{run_timeout} && $output_limit == $limits->{output_limit};
    for my $path ($cases_path, $config_path, abs_path($0)) {
        die "judge input absent from freeze: $path\n" unless exists $freeze->{identity_files}{$path};
        die "judge input changed: $path\n" unless $freeze->{identity_files}{$path} eq sha256_hex(bytes($path));
    }
    $qualification_path //= "$base/evidence/launch-qualification.json";
    $qualification_path = abs_path($qualification_path) or die "hidden judging blocked: qualification missing\n";
    my $qualified = $json->decode(bytes($qualification_path));
    my $qualified_sha = sha256_hex(bytes($qualification_path));
    die "hidden judging blocked: qualification is not for this freeze\n" unless ($qualified->{status} // '') eq 'qualified'
        && ($qualified->{kind} // '') eq 'yavo.executor.qualification'
        && ($qualified->{freeze_sha256} // '') eq $freeze_sha
        && ($qualified->{model_settings_sha256} // '') eq ($freeze->{model_settings_sha256} // '')
        && ($qualified->{executor_sha256} // '') eq ($freeze->{executor_sha256} // '');
    my %cohort;
    for my $run_id ('full-a', 'compact-a', 'compact-b', 'full-b') {
        my $root = "$base/runs/$run_id";
        die "hidden judging blocked: $run_id is missing\n" unless -f "$root/run.json" && -f "$root/submission.json" && -f "$root/events.jsonl";
        my $meta = $json->decode(bytes("$root/run.json"));
        my $submitted = $json->decode(bytes("$root/submission.json"));
        my ($arm, $replicate) = split /-/, $run_id;
        die "hidden judging blocked: $run_id is not sealed under the same freeze\n" unless ($meta->{status} // '') eq 'sealed'
            && ($meta->{frozen_manifest_sha256} // '') eq $freeze_sha && ($meta->{qualification_sha256} // '') eq $qualified_sha;
        for my $record ($meta, $submitted) {
            die "invalid cohort identity: $run_id\n" unless ($record->{schema_version} // 0) == 2
                && ($record->{run_id} // '') eq $run_id && ($record->{arm} // '') eq $arm
                && ($record->{replicate} // '') eq $replicate && ($record->{language} // '') eq 'yavo';
        }
        die "cohort model settings differ\n" unless ($meta->{model}{settings_sha256} // '') eq ($freeze->{model_settings_sha256} // '')
            && ($meta->{model}{exact_id} // '') eq ($freeze->{provider_policy}{expected_reported_model} // '');
        my @events = map { $json->decode($_) } grep { /\S/ } split /\n/, bytes("$root/events.jsonl");
        die "invalid sealed event sequence: $run_id\n" unless @events && @events == $meta->{operations}
            && $events[-1]{operation} eq 'seal' && $events[-1]{status} eq 'completed';
        my (%first, $sequence);
        $sequence = 0;
        for my $event (@events) {
            die "event sequence mismatch\n" unless $event->{sequence} == ++$sequence && $event->{run_id} eq $run_id;
            if ($event->{operation} eq 'build' && ($event->{build_number} // 0) == 1) {
                die "duplicate first build\n" if exists $first{$event->{task}};
                $first{$event->{task}} = $event->{source_manifest};
            }
        }
        die "cohort submission must contain T01..T08\n" unless ref($submitted->{tasks}) eq 'ARRAY' && @{$submitted->{tasks}} == 8;
        for my $i (1..8) {
            my $task = sprintf('T%02d', $i);
            my $item = $submitted->{tasks}[$i - 1];
            my $state = $meta->{tasks}{$task};
            die "task $task not sealed: $run_id\n" unless $item->{id} eq $task && ref($state) eq 'HASH'
                && JSON::PP::is_bool($state->{submitted}) && $state->{submitted};
            for my $stage ('first', 'final') {
                my $wanted = $stage eq 'first' ? "tasks/$task/snapshots/0001" : "tasks/$task/final";
                die "unexpected source-stage path\n" unless ($item->{$stage}{source_dir} // '') eq $wanted && ($item->{$stage}{entry} // '') eq 'main.yv';
                my $manifest = source_manifest(relative($root, $wanted), 'yavo')->{files};
                my $expected = $stage eq 'first' ? $first{$task} : $state->{final_manifest};
                die "missing or changed sealed source: $run_id $task $stage\n" unless ref($expected) eq 'HASH'
                    && exists($expected->{'main.yv'}) && $json->encode($manifest) eq $json->encode($expected);
            }
        }
        $cohort{$run_id} = {run_sha256 => sha256_hex(bytes("$root/run.json")),
                            submission_sha256 => sha256_hex(bytes("$root/submission.json")),
                            events_sha256 => sha256_hex(bytes("$root/events.jsonl")), status => 'sealed'};
    }
    die "submission is not one of the four sealed cohort paths\n" unless grep { $submission_path eq "$base/runs/$_/submission.json" } keys %cohort;
    return {freeze_sha256 => $freeze_sha, qualification_sha256 => $qualified_sha, all_runs => \%cohort,
            judge_limits => $limits, guard => 'all_four_sealed_same_freeze_and_source_identities'};
}

$cases_path = abs_path($cases_path) or die "missing cases\n";
$submission_path = abs_path($submission_path) or die "missing submission\n";
$config_path = abs_path($config_path) or die "missing compiler config\n";
my $cohort_guard = sealed_cohort();
die "output directory already exists; do not overwrite replay evidence\n" if -e $output;
make_path($output);
$output = abs_path($output) or die "missing output directory\n";
my $cases = $json->decode(bytes($cases_path));
my $submission = $json->decode(bytes($submission_path));
my $configuration = $json->decode(bytes($config_path));
my $language = $submission->{language};
die "invalid language\n" unless $language eq 'yavo';
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
my $report = {schema => 'yavo-r6-evaluation-v1', run_id => $submission->{run_id}, arm => $submission->{arm},
              language => $language, replicate => $submission->{replicate}, created_utc => utc(), cohort_guard => $cohort_guard,
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
