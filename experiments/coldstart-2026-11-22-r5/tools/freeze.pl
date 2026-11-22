#!/usr/bin/env perl
use strict;
use warnings;
use JSON::PP;
use Digest::SHA qw(sha256_hex);
use File::Find;
use File::Path qw(make_path);
use FindBin;
use Cwd qw(abs_path);
use POSIX qw(strftime);
my $base = abs_path("$FindBin::Bin/..");
my $repo = abs_path("$base/../..");
my $json = JSON::PP->new->utf8->canonical->pretty;
sub bytes { open my $f, '<:raw', $_[0] or die "$!: $_[0]"; local $/; my $s=<$f>; return defined($s) ? $s : ''; }
sub save { open my $f, '>:raw', $_[0] or die $!; print $f $json->encode($_[1]); close $f or die $!; }
sub output { open my $p, '-|', @_ or die $!; local $/; my $s=<$p>; close $p or die "command failed: @_"; return $s; }
die "frozen manifest already exists; do not overwrite\n" if -e "$base/evidence/frozen.json";
make_path("$base/evidence");
my $rustc = "$ENV{HOME}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc";
my $linker = "$ENV{HOME}/.yavo/bin/rust-linker";
my $yavo = "$repo/compiler/target/release/yavo";
my @tool_files = ($yavo, "$repo/compiler/runtime/yavo_rt.h", "$repo/compiler/toolchain.lock", $rustc, $linker);
my %identities = map { $_ => sha256_hex(bytes($_)) } @tool_files;
my $zig = "$ENV{HOME}/.yavo/toolchains/zig-x86_64-linux-0.17.1/zig";
my $sysroot = output($rustc, '--print', 'sysroot'); chomp $sysroot;
my @large_tool_files = ($zig, glob("$sysroot/lib/librustc_driver*.so"), glob("$sysroot/lib/rustlib/x86_64-unknown-linux-gnu/lib/*.rlib"));
my %toolchain = map { $_ => sha256_hex(bytes($_)) } @large_tool_files;
my %complete_identities = (%identities, %toolchain);
my $config = {
  yavo => { argv => [$yavo, 'build', '{entry}', '-o', '{output}'], env => {YAVO_ALLOC_CHECK => '1'}, identity_files => {%complete_identities} },
  rust => { argv => [$rustc, '--edition=2021', '-C', 'overflow-checks=yes', '-C', "linker=$linker", '{entry}', '-o', '{output}'], env => {}, identity_files => {%complete_identities} },
};
save("$base/evidence/compiler-config.json", $config);
my %files;
find({no_chdir=>1, wanted=>sub {
  return unless -f $_;
  my $rel=substr($File::Find::name, length($base)+1);
  return if $rel =~ m{\A(?:runs/|judge-results/|evidence/frozen\.json\z)};
  return if $rel =~ m{(?:^|/)__pycache__/} || $rel =~ /\.pyc\z/;
  $files{$rel}=sha256_hex(bytes($File::Find::name));
}}, $base);
for my $rel (keys %files) { $identities{"$base/$rel"} = $files{$rel}; }
my $revision = output('git','-C',$repo,'rev-parse','HEAD'); chomp $revision;
my $manifest = {
  schema_version=>1, created_utc=>strftime('%Y-%m-%dT%H:%M:%SZ',gmtime),
  user_date=>'2026-11-22', user_timezone=>'Asia/Shanghai', compiler_revision=>$revision,
  last_language_code_revision=>'8a65228f31139a4b07e256fec70ccd6aaff06176',
  yavo_binary_sha256=>$identities{$yavo}, rustc_verbose=>output($rustc,'-Vv'),
  rustc_binary_sha256=>$identities{$rustc}, files=>\%files, identity_files=>\%identities,
  toolchain_files=>\%toolchain, zig_version=>output($zig,'version'), rust_sysroot=>$sysroot,
  identity_check_scope=>'Core identities and frozen inputs before each build; Zig executable, Rust driver and Rust standard-library archives at session start/end and judge start/end. Full Zig library tree is represented by the pinned toolchain archive lock, not rehashed here.',
  immutable_statement=>'Created before participant sessions; SHA checks run before each build. Subsequent documentation commits do not change language implementation.',
};
save("$base/evidence/frozen.json", $manifest);
print $json->encode({frozen_files=>scalar(keys %files), compiler_revision=>$revision, manifest_sha256=>sha256_hex(bytes("$base/evidence/frozen.json"))});
