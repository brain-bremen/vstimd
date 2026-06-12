%global cargo_version %(grep '^version' ../../server/Cargo.toml 2>/dev/null | head -1 | sed 's/version = "\\(.*\\)"/\\1/' || echo 0.0.0)

Name:           vstimd
# Pass version at build time: rpmbuild -bb --define "pkg_version X.Y.Z" ...
# Falls back to reading server/Cargo.toml relative to the spec file.
Version:        %{?pkg_version}%{!?pkg_version:%{cargo_version}}
Release:        1%{?dist}
Summary:        Visual stimulus display server for neuroscience experiments
License:        AGPL-3.0-or-later
URL:            https://github.com/braemons/vstimd

# The binary is pre-built; this spec does not compile from source.
# Build with: cargo build --release [--target <triple>]
# Then: rpmbuild -bb packaging/rpm/vstimd.spec \
#           --define "_sourcedir $(pwd)/target/release"

%description
vstimd drives a display directly via VK_KHR_display without a compositor,
providing sub-millisecond frame timing for psychophysics experiments.

Communicates via ZMQ (port 5555) using a protobuf protocol. Supports
Jetson Orin Nano, Raspberry Pi 4/5, and desktop NVIDIA/AMD GPUs.

%install
make install DESTDIR=%{buildroot} PREFIX=%{_prefix} UNITDIR=%{_unitdir}

%post
%sysusers_create_package vstimd %{_sysusersdir}/vstimd.conf
%systemd_post vstimd.service

%preun
%systemd_preun vstimd.service

%postun
%systemd_postun_with_restart vstimd.service

%files
%{_bindir}/vstimd
%{_unitdir}/vstimd.service
%{_sysusersdir}/vstimd.conf
