Name:           vstimd
# Pass version at build time: rpmbuild -bb --define "pkg_version X.Y.Z" ...
Version:        %{?pkg_version}%{!?pkg_version:0.0.0}
Release:        1%{?dist}
Summary:        Visual stimulus display server for neuroscience experiments
License:        AGPL-3.0-or-later
URL:            https://github.com/braemons/vstimd

BuildRequires:  systemd-rpm-macros

# The binary is pre-built; this spec does not compile from source.
# Build with: cargo build --release [--target <triple>]
# Docker: packaging/docker/Dockerfile.rpm-builder handles compilation and packaging.
# Manual: rpmbuild -bb packaging/rpm/vstimd.spec \
#             --define "_builddir $(pwd)" \
#             --define "pkg_version $(grep '^version' server/Cargo.toml | head -1 | sed 's/version = \"\(.*\)\"/\1/')"

%description
vstimd drives a display directly via VK_KHR_display without a compositor,
providing sub-millisecond frame timing for psychophysics experiments.

Communicates via ZMQ (port 5555) using a protobuf protocol. Supports
Jetson Orin Nano, Raspberry Pi 4/5, and desktop NVIDIA/AMD GPUs.

%install
install -D -m 0755 %{_builddir}/target/release/vstimd                    %{buildroot}%{_bindir}/vstimd
install -D -m 0755 %{_builddir}/packaging/scripts/vstimd-boot-entry      %{buildroot}%{_sbindir}/vstimd-boot-entry
install -D -m 0644 %{_builddir}/packaging/systemd/vstimd.service         %{buildroot}%{_unitdir}/vstimd.service
install -D -m 0644 %{_builddir}/packaging/systemd/vstimd.target          %{buildroot}%{_unitdir}/vstimd.target
install -D -m 0644 %{_builddir}/packaging/sysusers/vstimd.conf           %{buildroot}%{_sysusersdir}/vstimd.conf
install -D -m 0644 %{_builddir}/packaging/rsyslog/vstimd.conf             %{buildroot}%{_sysconfdir}/rsyslog.d/10-vstimd.conf
install -D -m 0644 %{_builddir}/packaging/logrotate/vstimd                %{buildroot}%{_sysconfdir}/logrotate.d/vstimd

%post
%sysusers_create_package vstimd %{_sysusersdir}/vstimd.conf
%systemd_post vstimd.service
# Create log directory (rsyslog writes here as root).
install -d -m 0755 /var/log/vstimd
# Reload rsyslog if installed so it picks up the new drop-in.
if systemctl is-active --quiet rsyslog.service 2>/dev/null; then
    systemctl reload rsyslog.service || true
fi
# Register the "Boot to vstimd" bootloader entry; non-fatal on failure.
%{_sbindir}/vstimd-boot-entry 2>&1 | sed 's/^/vstimd: /' || true

%preun
%systemd_preun vstimd.service
# Remove the boot entry before the binary is erased.
if [ $1 -eq 0 ]; then
    %{_sbindir}/vstimd-boot-entry --remove 2>&1 | sed 's/^/vstimd: /' || true
fi

%postun
%systemd_postun_with_restart vstimd.service

%files
%{_bindir}/vstimd
%{_sbindir}/vstimd-boot-entry
%{_unitdir}/vstimd.service
%{_unitdir}/vstimd.target
%{_sysusersdir}/vstimd.conf
%config(noreplace) %{_sysconfdir}/rsyslog.d/10-vstimd.conf
%config(noreplace) %{_sysconfdir}/logrotate.d/vstimd
%dir /var/log/vstimd
