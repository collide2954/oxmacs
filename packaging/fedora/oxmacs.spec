%global debug_package %{nil}
%global app_id io.github.collide2954.oxmacs

Name:           oxmacs
Version:        %{oxmacs_version}
Release:        1%{?dist}
Summary:        An Emacs-inspired editor, extended in Ox
License:        Apache-2.0
URL:            https://github.com/collide2954/oxmacs
ExclusiveArch:  x86_64

Requires:       alsa-lib
Requires:       libX11
Requires:       libXcursor
Requires:       libXi
Requires:       libXrandr
Requires:       libwayland-client
Requires:       libwayland-cursor
Requires:       libxkbcommon
Requires:       libxkbcommon-x11
Requires:       mesa-libEGL
Requires:       vulkan-loader
Recommends:     mesa-vulkan-drivers
Recommends:     git-core
Recommends:     openssh-clients
Recommends:     curl

%description
oxmacs feels like Emacs to use: the same keys, a minibuffer, M-x, buffers,
windows and special buffers such as dired. Underneath it is a native Rust
core drawn with Iced, extended in Ox, its own modern Scheme. Dired, git,
an IRC client, a calendar, a password vault, a remote file browser and the
Ox manual come as bundled Ox plugins. The package also installs ox, which
runs, checks, formats and tests Ox code.

%install
install -Dpm 0755 %{oxmacs_source}/target/release/oxmacs %{buildroot}%{_bindir}/oxmacs
install -Dpm 0755 %{oxmacs_source}/target/release/ox %{buildroot}%{_bindir}/ox
install -Dpm 0644 %{oxmacs_source}/packaging/linux/%{app_id}.desktop %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -Dpm 0644 %{oxmacs_source}/LICENSE-APACHE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE-APACHE
install -Dpm 0644 %{oxmacs_source}/THIRD-PARTY-LICENSES.html %{buildroot}%{_datadir}/licenses/%{name}/THIRD-PARTY-LICENSES.html
install -Dpm 0644 %{oxmacs_source}/crates/oxmacs-gui/fonts/JetBrainsMono-OFL.txt %{buildroot}%{_datadir}/licenses/%{name}/JetBrainsMono-OFL.txt
install -Dpm 0644 %{oxmacs_source}/crates/oxmacs-core/queries/NOTICE.md %{buildroot}%{_datadir}/licenses/%{name}/QUERIES-NOTICE.md
install -Dpm 0644 %{oxmacs_source}/README.md %{buildroot}%{_docdir}/%{name}/README.md
install -Dpm 0644 %{oxmacs_source}/CHANGELOG.md %{buildroot}%{_docdir}/%{name}/CHANGELOG.md

%files
%license %{_datadir}/licenses/%{name}/LICENSE-APACHE
%license %{_datadir}/licenses/%{name}/THIRD-PARTY-LICENSES.html
%license %{_datadir}/licenses/%{name}/JetBrainsMono-OFL.txt
%license %{_datadir}/licenses/%{name}/QUERIES-NOTICE.md
%doc %{_docdir}/%{name}/README.md
%doc %{_docdir}/%{name}/CHANGELOG.md
%{_bindir}/oxmacs
%{_bindir}/ox
%{_datadir}/applications/%{app_id}.desktop
