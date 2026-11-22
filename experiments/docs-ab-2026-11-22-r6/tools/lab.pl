#!/usr/bin/env perl
use strict;
use warnings;
use JSON::PP;
use Digest::SHA qw(sha256_hex);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC sleep);
use POSIX qw(strftime WNOHANG);
use File::Path qw(make_path);
use File::Copy qw(copy);
use File::Find;
use File::Spec;
use Cwd qw(abs_path);
use Fcntl qw(:flock);
use FindBin;
use Encode qw(decode encode FB_CROAK);

# The participant interface exposes only public inputs and its own sources.
# This is an instrumented protocol boundary, not an OS security sandbox.
my $BASE = abs_path("$FindBin::Bin/..");
my $ROOT = abs_path("$BASE/../..");
my $QUALIFICATION_PATH = $ENV{YAVO_QUALIFICATION_RECEIPT} // "$BASE/evidence/launch-qualification.json";
my $JSON = JSON::PP->new->canonical->utf8;
my ($run_id, $op, @args) = @ARGV;
die "usage: lab.pl RUN OP [TASK] [FILE]\n" unless defined $op;
die "invalid run id\n" unless $run_id =~ /\A(?:(?:full|compact)-(?:a|b)|probe)\z/;
my $R = "$BASE/runs/$run_id";
my $meta;
my $event;
my $started = clock_gettime(CLOCK_MONOTONIC);
my $operation_stdout = '';

sub now { return strftime('%Y-%m-%dT%H:%M:%SZ', gmtime); }
sub read_bytes {
    my ($path) = @_;
    open my $f, '<:raw', $path or die "read $path: $!\n";
    local $/;
    my $s = <$f>;
    close $f;
    return defined $s ? $s : '';
}
sub write_bytes {
    my ($path, $s) = @_;
    my ($vol, $dir) = File::Spec->splitpath($path);
    make_path($dir);
    open my $f, '>:raw', "$path.tmp" or die "write $path: $!\n";
    print $f $s;
    close $f or die "close $path: $!\n";
    rename "$path.tmp", $path or die "rename $path: $!\n";
}
sub read_json { return $JSON->decode(read_bytes($_[0])); }
sub write_json { write_bytes($_[0], $JSON->pretty->encode($_[1])); $JSON->indent(0); }
sub emit { my $s = ref($_[0]) ? $JSON->pretty->encode($_[0]) : $_[0]; $JSON->indent(0); $operation_stdout .= $s; print $s; }
sub task_id {
    my ($t) = @_;
    die "task must be T01..T08\n" unless defined $t && $t =~ /\AT0[1-8]\z/;
    my ($current) = grep { !exists($meta->{tasks}{$_}) || !$meta->{tasks}{$_}{submitted} } map {sprintf('T%02d',$_)} 1..8;
    die "complete tasks in order; current task is " . ($current // 'none') . "\n" unless defined($current) && $current eq $t;
    return $t;
}
sub source_name {
    my ($p) = @_;
    die "invalid relative source path\n" unless defined $p && $p =~ /\A[A-Za-z0-9_-]+(?:\/[A-Za-z0-9_-]+)*\.(?:yv|rs)\z/;
    my $want = $meta->{language} eq 'yavo' ? '.yv' : '.rs';
    die "wrong source extension\n" unless substr($p, -length($want)) eq $want;
    return $p;
}
sub source_manifest {
    my ($dir) = @_;
    my %files;
    return \%files unless -d $dir;
    find({no_chdir => 1, wanted => sub {
        die "symlink in participant sources\n" if -l $_;
        return unless -f $_;
        my $rel = substr($File::Find::name, length($dir) + 1);
        source_name($rel);
        $files{$rel} = sha256_hex(read_bytes($File::Find::name));
    }}, $dir);
    return \%files;
}
sub copy_sources {
    my ($from, $to) = @_;
    die "snapshot already exists\n" if -e $to;
    make_path($to);
    my $files = source_manifest($from);
    for my $rel (sort keys %$files) { write_bytes("$to/$rel", read_bytes("$from/$rel")); }
    return $files;
}
sub assert_identity {
    my ($include_large_toolchain) = @_;
    my $identity = read_json("$BASE/evidence/frozen.json");
    die "invalid experiment freeze\n" unless ($identity->{kind} // '') eq 'yavo.docs_ab.freeze' && ($identity->{experiment_id} // '') eq 'docs-ab-2026-11-22-r6';
    die "missing identity inventory\n" unless ref($identity->{identity_files}) eq 'HASH' && keys %{$identity->{identity_files}};
    die "missing toolchain inventory\n" unless ref($identity->{toolchain_files}) eq 'HASH' && keys %{$identity->{toolchain_files}};
    for my $required ("$ROOT/compiler/target/release/yavo", "$ROOT/compiler/runtime/yavo_rt.h", "$ROOT/compiler/toolchain.lock", "$BASE/tools/lab.pl", "$BASE/documents.json", "$BASE/public/cases.json", "$BASE/judge/cases.json", "$ROOT/tools/agent_eval/executor.py") {
        die "missing required identity: $required\n" unless exists($identity->{identity_files}{$required});
    }
    for my $p (sort keys %{$identity->{identity_files}}) {
        die "invalid frozen digest: $p\n" unless $identity->{identity_files}{$p} =~ /\A[a-f0-9]{64}\z/;
        die "frozen identity changed: $p\n" unless sha256_hex(read_bytes($p)) eq $identity->{identity_files}{$p};
    }
    if ($include_large_toolchain) {
        for my $p (sort keys %{$identity->{toolchain_files} // {}}) {
            die "frozen toolchain changed: $p\n" unless sha256_hex(read_bytes($p)) eq $identity->{toolchain_files}{$p};
        }
    }
    return $identity;
}
sub run_command {
    my ($argv, $cwd, $input, $dir, $limit, $env) = @_;
    make_path($dir);
    write_bytes("$dir/stdin.txt", $input);
    my $start = clock_gettime(CLOCK_MONOTONIC);
    my $pid = fork();
    die "fork: $!\n" unless defined $pid;
    if ($pid == 0) {
        eval {
            POSIX::setpgid(0, 0);
            chdir $cwd or die "chdir: $!";
            open STDIN, '<:raw', "$dir/stdin.txt" or die $!;
            open STDOUT, '>:raw', "$dir/stdout.txt" or die $!;
            open STDERR, '>:raw', "$dir/stderr.txt" or die $!;
            $ENV{PATH} = "$ENV{HOME}/.cargo/bin:$ENV{PATH}";
            delete @ENV{qw(YAVO_CC YAVO_UPDATE_BASELINE YAVO_NO_CACHE YAVO_CACHE_TRACE)};
            for my $k (keys %$env) { $ENV{$k} = $env->{$k}; }
            exec {$argv->[0]} @$argv;
            die "exec: $!";
        };
        print STDERR "launcher error: $@\n";
        POSIX::_exit(127);
    }
    my ($status, $timed_out, $output_limit) = (undef, 0, 0);
    my $max_output = 1048576;
    while (1) {
        my $got = waitpid($pid, WNOHANG);
        if ($got == $pid) { $status = $?; last; }
        die "waitpid: $!\n" if $got < 0;
        $output_limit = ((-s "$dir/stdout.txt" // 0) > $max_output || (-s "$dir/stderr.txt" // 0) > $max_output) ? 1 : 0;
        if (clock_gettime(CLOCK_MONOTONIC) - $start > $limit || $output_limit) {
            $timed_out = $output_limit ? 0 : 1;
            kill 'TERM', -$pid;
            sleep(0.2);
            kill 'KILL', -$pid;
            waitpid($pid, 0);
            $status = $?;
            last;
        }
        sleep(0.02);
    }
    kill 'KILL', -$pid;
    my %captured;
    for my $stream ('stdout', 'stderr') {
        my $path = "$dir/$stream.txt";
        write_bytes($path, '') unless -f $path;
        open my $out, '<:raw', $path or die $!;
        read($out, my $part, $max_output);
        close $out;
        $captured{$stream} = $part // '';
        $output_limit = 1 if (-s $path) > $max_output;
    }
    my $result = {
        argv => $argv, cwd => $cwd, env => $env, timeout_seconds => $limit,
        started_monotonic => $start, elapsed_seconds => clock_gettime(CLOCK_MONOTONIC) - $start,
        exit_code => ($status & 127) ? undef : ($status >> 8),
        signal => $status & 127, timed_out => $timed_out ? JSON::PP::true : JSON::PP::false,
        output_limit => $output_limit ? JSON::PP::true : JSON::PP::false,
        stdout => $captured{stdout}, stderr => $captured{stderr},
        stdout_sha256 => sha256_hex($captured{stdout}), stderr_sha256 => sha256_hex($captured{stderr}),
        capture_bytes_limit => $max_output,
    };
    # Task data are UTF-8. JSON::PP consumes UTF-8 bytes when decoding, but scalar
    # strings must be decoded before encoding to avoid double-encoding stdout.
    require Encode;
    $result->{stdout} = Encode::decode('UTF-8', $result->{stdout}, Encode::FB_DEFAULT());
    $result->{stderr} = Encode::decode('UTF-8', $result->{stderr}, Encode::FB_DEFAULT());
    write_json("$dir/result.json", $result);
    return $result;
}
sub successful { return !$_[0]{timed_out} && !$_[0]{output_limit} && !$_[0]{signal} && defined $_[0]{exit_code} && $_[0]{exit_code} == 0; }
sub entry { return $meta->{language} eq 'yavo' ? 'main.yv' : 'main.rs'; }
sub tstate {
    my ($t) = @_;
    $meta->{tasks}{$t} //= {builds => 0, submitted => JSON::PP::false};
    return $meta->{tasks}{$t};
}
sub no_extra { die "unexpected argument\n" if @args > $_[0]; }

sub document_pages {
    my ($bytes, $limit) = @_;
    die "invalid document page size\n" unless $limit == 8192;
    my $content = decode('UTF-8', $bytes, FB_CROAK);
    my @pages;
    my $page = '';
    for my $char (split //, $content) {
        my $encoded = encode('UTF-8', $char);
        if (length($page) + length($encoded) > $limit) {
            push @pages, $page;
            $page = '';
        }
        $page .= $encoded;
    }
    push @pages, $page if length($page) || !@pages;
    return \@pages;
}
sub document_path {
    my ($name) = @_;
    my $catalog = read_json("$BASE/documents.json");
    die "unknown document\n" unless defined($name) && exists($catalog->{documents}{$name});
    my $item = $catalog->{documents}{$name};
    my $relative = ref($item->{path}) ? $item->{path}{$meta->{arm}} : $item->{path};
    die "unsafe document path\n" unless defined($relative) && $relative =~ /\A[A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*\z/ && !grep { $_ eq '..' || $_ eq '.' } split('/', $relative);
    die "document symlink\n" if -l "$BASE/$relative";
    return ($relative, $item->{category}, $catalog->{page_bytes});
}
sub require_initial_documents {
    for my $name ('guide', 'index') {
        my ($relative, undef, $limit) = document_path($name);
        my $pages = document_pages(read_bytes("$BASE/$relative"), $limit);
        for my $page (1..scalar(@$pages)) {
            die "read all initial guide and index pages before building: $name page $page\n" unless $meta->{document_pages}{$name}{$page};
        }
    }
}
sub qualification {
    my $path = $QUALIFICATION_PATH;
    die "experiment launch blocked: missing executor qualification\n" unless -f $path;
    my $q = read_json($path);
    die "experiment launch blocked: executor is not qualified\n" unless ($q->{status} // '') eq 'qualified';
    die "qualification is for another experiment\n" unless ($q->{kind} // '') eq 'yavo.executor.qualification' && ($q->{experiment_id} // '') eq 'docs-ab-2026-11-22-r6';
    for my $key ('freeze_sha256', 'model_settings_sha256', 'executor_sha256') {
        die "qualification missing $key\n" unless ($q->{$key} // '') =~ /\A[a-f0-9]{64}\z/;
    }
    die "qualification freeze mismatch\n" unless $q->{freeze_sha256} eq sha256_hex(read_bytes("$BASE/evidence/frozen.json"));
    my $frozen = read_json("$BASE/evidence/frozen.json");
    die "qualification settings mismatch\n" unless $q->{model_settings_sha256} eq ($frozen->{model_settings_sha256} // '');
    die "qualification executor mismatch\n" unless $q->{executor_sha256} eq ($frozen->{executor_sha256} // '') && $q->{executor_sha256} eq sha256_hex(read_bytes("$ROOT/tools/agent_eval/executor.py"));
    my $observed = $q->{observed};
    die "qualification missing provider evidence\n" unless ref($observed) eq 'HASH' && ($observed->{provider_requests} // 0) > 0 && ($observed->{provider_responses} // 0) > 0;
    die "qualification missing live tool round trip\n" unless ($observed->{provider_responses} // 0) >= 2 && ($observed->{qualifying_tool_calls} // 0) >= 1;
    for my $field ('usage_complete', 'tool_accounting_complete', 'per_request_evidence') {
        die "qualification incomplete: $field\n" unless JSON::PP::is_bool($observed->{$field}) && $observed->{$field};
    }
    die "qualification missing reported model\n" unless ref($observed->{reported_models}) eq 'ARRAY' && @{$observed->{reported_models}} == 1 && length($observed->{reported_models}[0]);
    return $q;
}

if ($run_id eq 'probe') {
    die "only the fixed metering probe is permitted without qualification\n" unless $op eq 'doc' && @args == 2 && $args[0] eq 'meter-probe' && $args[1] eq '1';
    assert_identity(1);
    my $raw = read_bytes("$BASE/METER_PROBE.txt");
    emit({document => 'meter-probe', page => 1, total_pages => 1,
          bytes => length($raw), sha256 => sha256_hex($raw), content => decode('UTF-8', $raw, FB_CROAK)});
    exit 0;
}

if ($op eq 'init') {
    no_extra(0);
    die "run already initialized\n" if -e "$R/run.json";
    my $qualified = qualification();
    my $frozen = assert_identity(1);
    make_path($R);
    my ($arm, $replicate) = split /-/, $run_id;
    $meta = {schema_version => 2, run_id => $run_id, language => 'yavo', arm => $arm, replicate => $replicate,
        status => 'active', started_at => now(), started_monotonic => $started,
        model => {exact_id => $qualified->{observed}{reported_models}[0], settings_sha256 => $qualified->{model_settings_sha256}},
        usage => undef, usage_status => 'pending_executor_transcript',
        qualification_sha256 => sha256_hex(read_bytes($QUALIFICATION_PATH)),
        compiler_revision => $frozen->{compiler_revision}, frozen_manifest_sha256 => sha256_hex(read_bytes("$BASE/evidence/frozen.json")),
        tasks => {}, operations => 0, operations_by_category => {}, document_pages => {},
        measurement => 'lab operations and bytes; provider calls and usage are joined from the executor transcript',
        time_scope => 'controller init through participant seal; final response and provider end-to-end timing are recorded separately',
    };
    write_json("$R/run.json", $meta);
    emit({run_id => $run_id, status => 'initialized', started_at => $meta->{started_at}});
    exit 0;
}

die "run not initialized\n" unless -f "$R/run.json";
open my $lock, '>>', "$R/.lock" or die $!;
flock($lock, LOCK_EX) or die $!;
$meta = read_json("$R/run.json");
die "qualification receipt changed during session\n" unless -f $QUALIFICATION_PATH && sha256_hex(read_bytes($QUALIFICATION_PATH)) eq $meta->{qualification_sha256};
die "experiment freeze changed during session\n" unless sha256_hex(read_bytes("$BASE/evidence/frozen.json")) eq $meta->{frozen_manifest_sha256};
assert_identity();
die "run is already sealed\n" if $meta->{status} eq 'sealed' && $op ne 'controller-note';
my $seq = $meta->{operations} + 1;
$event = {schema_version => 2, sequence => $seq, run_id => $run_id, operation => $op,
    arguments => [map { "$_" } @args], started_at => now(), started_monotonic => $started,
    category => 'control', status => 'started'};
write_json(sprintf("$R/operations/%04d/request.json", $seq), $event);
my $error;
eval {
    if ($op eq 'doc') {
        die "doc requires DOCUMENT PAGE\n" unless @args == 2 && $args[1] =~ /\A[1-9][0-9]*\z/;
        my ($relative, $category, $limit) = document_path($args[0]);
        my $document = read_bytes("$BASE/$relative");
        my $pages = document_pages($document, $limit);
        my $page = 0 + $args[1];
        die "document page out of range\n" if $page > @$pages;
        my $content = $pages->[$page - 1];
        my $repeated = $meta->{document_pages}{$args[0]}{$page} ? 1 : 0;
        my $offset = 0;
        for (my $index = 0; $index < $page - 1; $index++) { $offset += length($pages->[$index]); }
        $event->{category} = $repeated ? 'repeated_read' : $category;
        $event->{document_category} = $category;
        $event->{repeated_read} = $repeated ? JSON::PP::true : JSON::PP::false;
        $event->{document} = $args[0];
        $event->{document_sha256} = sha256_hex($document);
        $event->{page} = $page; $event->{total_pages} = scalar(@$pages);
        $event->{document_bytes_returned} = length($content);
        $event->{byte_start} = $offset;
        $event->{byte_end_exclusive} = $offset + length($content);
        $event->{document_page_sha256} = sha256_hex($content);
        $meta->{document_pages}{$args[0]}{$page} = JSON::PP::true;
        emit({document => $args[0], page => $page, total_pages => scalar(@$pages),
              byte_start => $offset, byte_end_exclusive => $offset + length($content),
              bytes => length($content), content => decode('UTF-8', $content, FB_CROAK)});
    } elsif ($op eq 'write') {
        no_extra(2);
        my $t = task_id($args[0]); my $p = source_name($args[1]); my $s = tstate($t);
        die "task submitted\n" if $s->{submitted};
        local $/; my $bytes = <STDIN>; $bytes //= '';
        die "source exceeds 256KiB\n" if length($bytes) > 262144;
        my $old = -f "$R/tasks/$t/work/$p" ? sha256_hex(read_bytes("$R/tasks/$t/work/$p")) : undef;
        write_bytes("$R/tasks/$t/work/$p", $bytes);
        write_bytes(sprintf("$R/operations/%04d/input-source.txt", $seq), $bytes);
        $event->{category} = $s->{last_build_failed} ? 'repair_after_compile_failure' : $s->{last_public_failed} ? 'repair_after_public_failure' : 'write_code';
        $event->{source_path} = $p; $event->{previous_sha256} = $old; $event->{sha256} = sha256_hex($bytes); $event->{bytes} = length($bytes);
        $s->{source_changed} = JSON::PP::true;
        emit({written => "$t/$p", sha256 => $event->{sha256}, bytes => length($bytes)});
    } elsif ($op eq 'read') {
        no_extra(2);
        my $t = task_id($args[0]); my $p = source_name($args[1]);
        $event->{category} = 'read_own_code';
        emit(read_bytes("$R/tasks/$t/work/$p"));
    } elsif ($op eq 'build') {
        no_extra(1); my $t = task_id($args[0]); my $s = tstate($t);
        require_initial_documents();
        die "task submitted\n" if $s->{submitted};
        die "eight-build task budget exhausted\n" if $s->{builds} >= 8;
        assert_identity();
        my $present = source_manifest("$R/tasks/$t/work");
        die "missing entry source\n" unless exists $present->{entry()};
        my $config = read_json("$BASE/evidence/compiler-config.json")->{$meta->{language}};
        die "invalid compiler command\n" unless ref($config) eq 'HASH' && ref($config->{argv}) eq 'ARRAY' && @{$config->{argv}};
        my $n = ++$s->{builds}; my $id = sprintf('%04d', $n);
        if ($n == 1) { $s->{first_build_passed} = JSON::PP::false; $s->{first_build_exit_code} = undef; }
        my $snapshot = "$R/tasks/$t/snapshots/$id";
        my $files = copy_sources("$R/tasks/$t/work", $snapshot);
        die "missing entry source\n" unless exists $files->{entry()};
        my $build = "$R/tasks/$t/builds/$id"; make_path($build);
        my %sub = (entry => "$snapshot/" . entry(), output => "$build/program", source_dir => $snapshot);
        my @argv = map { my $a=$_; $a =~ s/\{(entry|output|source_dir)\}/$sub{$1}/ge; $a } @{$config->{argv}};
        my %env = %{$config->{env} // {}};
        $env{YAVO_CACHE_DIR} = "$R/cache" if $meta->{language} eq 'yavo';
        $event->{category} = 'compile'; $event->{task} = $t; $event->{build_number} = $n; $event->{source_manifest} = $files;
        my $result = run_command(\@argv, $snapshot, '', $build, 120, \%env);
        $event->{result} = $result;
        $s->{last_build_failed} = successful($result) ? JSON::PP::false : JSON::PP::true;
        $s->{last_public_failed} = JSON::PP::false;
        $s->{source_changed} = JSON::PP::false;
        $s->{latest_build} = $id;
        $s->{latest_build_manifest} = $files;
        $s->{first_build_exit_code} = $result->{exit_code} if $n == 1;
        $s->{first_build_passed} = successful($result) ? JSON::PP::true : JSON::PP::false if $n == 1;
        emit({task => $t, build => $n, source_manifest => $files, %$result});
    } elsif ($op eq 'public' || $op eq 'run') {
        no_extra(1); my $t = task_id($args[0]); my $s = tstate($t);
        die "build current sources successfully first\n" if !$s->{latest_build} || $s->{last_build_failed} || $s->{source_changed};
        die "source bytes differ from latest snapshot\n" unless $JSON->encode(source_manifest("$R/tasks/$t/work")) eq $JSON->encode($s->{latest_build_manifest});
        my $program = "$R/tasks/$t/builds/$s->{latest_build}/program";
        $event->{task} = $t; $event->{build_number} = int($s->{latest_build});
        $event->{category} = $op eq 'public' ? 'test_public' : 'test_custom';
        my @cases;
        if ($op eq 'public') {
            my $data = read_json("$BASE/public/cases.json");
            my ($task) = grep { $_->{id} eq $t } @{$data->{tasks}};
            die "public task missing\n" unless $task;
            @cases = @{$task->{cases}};
            die "nonpublic case in public interface\n" if grep { $_->{visibility} ne 'public' } @cases;
        } else {
            local $/; my $input = <STDIN>; $input //= '';
            require Encode;
            @cases = ({id => 'custom', stdin => Encode::decode('UTF-8', $input), args => []});
        }
        my @results; my $all = 1;
        for my $case (@cases) {
            die "invalid public case id\n" unless $case->{id} =~ /\A[A-Za-z0-9_-]+\z/;
            my $dir = sprintf("$R/operations/%04d/cases/%s", $seq, $case->{id});
            my @argv = ($program, @{$case->{args} // []});
            require Encode;
            my $input = Encode::encode('UTF-8', $case->{stdin});
            my $r = run_command(\@argv, "$R/tasks/$t/work", $input, $dir, 5, {});
            my $passed;
            if ($op eq 'public') {
                $passed = !$r->{timed_out} && !$r->{output_limit} && !$r->{signal} && defined($r->{exit_code}) && $r->{exit_code} == $case->{exit_code} && read_bytes("$dir/stdout.txt") eq Encode::encode('UTF-8', $case->{stdout}) && read_bytes("$dir/stderr.txt") eq Encode::encode('UTF-8', $case->{stderr});
                $all &&= $passed;
            }
            push @results, {id => $case->{id}, observed => $r,
                ($op eq 'public' ? (passed => $passed ? JSON::PP::true : JSON::PP::false, expected => {stdout => $case->{stdout}, stderr => $case->{stderr}, exit_code => $case->{exit_code}}) : ())};
        }
        $event->{results} = \@results;
        $s->{last_public_failed} = $all ? JSON::PP::false : JSON::PP::true if $op eq 'public';
        $s->{public_build} = $s->{latest_build} if $op eq 'public';
        emit({task => $t, build => int($s->{latest_build}), cases => \@results});
    } elsif ($op eq 'submit') {
        no_extra(1); my $t = task_id($args[0]); my $s = tstate($t);
        die "task already submitted\n" if $s->{submitted};
        die "at least one build attempt required\n" unless $s->{builds};
        die "no completed compiler invocation to submit\n" unless $s->{latest_build} && -f "$R/tasks/$t/builds/$s->{latest_build}/result.json";
        die "build the current source before submit\n" if $s->{source_changed};
        die "source bytes differ from latest snapshot\n" unless $JSON->encode(source_manifest("$R/tasks/$t/work")) eq $JSON->encode($s->{latest_build_manifest});
        die "run public samples for the current successful build before submit\n" if !$s->{last_build_failed} && ($s->{public_build} // '') ne ($s->{latest_build} // '');
        my $files = copy_sources("$R/tasks/$t/work", "$R/tasks/$t/final");
        $s->{submitted} = JSON::PP::true; $s->{final_manifest} = $files;
        $event->{category} = 'submit'; $event->{task} = $t; $event->{source_manifest} = $files;
        emit({task => $t, status => 'sealed', final_manifest => $files});
    } elsif ($op eq 'seal') {
        no_extra(0);
        assert_identity(1);
        my @tasks;
        for my $i (1..8) {
            my $t = sprintf('T%02d', $i); my $s = tstate($t);
            die "task $t has not been submitted\n" unless $s->{submitted};
            push @tasks, {id => $t, first => {source_dir => "tasks/$t/snapshots/0001", entry => entry()}, final => {source_dir => "tasks/$t/final", entry => entry()}};
        }
        $meta->{status} = 'sealed';
        $meta->{ended_at} = now(); $meta->{ended_monotonic} = clock_gettime(CLOCK_MONOTONIC);
        $meta->{elapsed_seconds} = $meta->{ended_monotonic} - $meta->{started_monotonic};
        write_json("$R/submission.json", {schema_version => 2, run_id => $run_id, language => $meta->{language}, arm => $meta->{arm}, replicate => $meta->{replicate}, tasks => \@tasks});
        $event->{category} = 'submit';
        emit({run_id => $run_id, status => 'sealed', elapsed_seconds => $meta->{elapsed_seconds}, usage => undef});
    } elsif ($op eq 'controller-note') {
        die "controller-note is outside participant protocol\n";
    } else { die "unknown operation\n"; }
    1;
} or $error = $@ || 'unknown error';
$event->{ended_at} = now();
$event->{elapsed_seconds} = clock_gettime(CLOCK_MONOTONIC) - $started;
$event->{status} = defined $error ? 'interface_error' : 'completed';
$event->{error} = $error if defined $error;
$meta->{operations} = $seq;
$meta->{operations_by_category}{$event->{category}}++;
write_bytes(sprintf("$R/operations/%04d/output.txt", $seq), $operation_stdout);
write_json(sprintf("$R/operations/%04d/event.json", $seq), $event);
open my $events, '>>:raw', "$R/events.jsonl" or die $!;
print $events $JSON->encode($event), "\n";
close $events;
write_json("$R/run.json", $meta);
if (defined $error) { print STDERR $error; exit 2; }
exit 0;
