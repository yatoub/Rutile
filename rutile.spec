Name:           rutile
Version:        0.1.0
Release:        1%{?dist}
Summary:        GNOME-native terminal emulator with split tiling and synchronized input
License:        MIT
URL:            https://github.com/yatoub/Rutile
Source0:        https://github.com/yatoub/Rutile/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  vte291-devel
BuildRequires:  pkgconf-pkg-config
BuildRequires:  desktop-file-utils

%description
Rutile is a from-scratch Rust/GTK4 rewrite of Tilix, aiming for functional
parity on its core value: recursive split tiling, synchronized input
across panes, independent multi-session support, and a Catppuccin theme.

%prep
%autosetup -n Rutile-%{version}
export RUSTUP_TOOLCHAIN=stable

%build
export RUSTUP_TOOLCHAIN=stable
cargo build --frozen --release

%check
export RUSTUP_TOOLCHAIN=stable
cargo test --frozen

%install
install -Dm0755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
install -Dm0644 resources/%{name}.desktop %{buildroot}%{_datadir}/applications/%{name}.desktop
desktop-file-validate %{buildroot}%{_datadir}/applications/%{name}.desktop
install -Dm0644 README.md %{buildroot}%{_docdir}/%{name}/README.md

%files
%license LICENSE
%{_bindir}/%{name}
%{_datadir}/applications/%{name}.desktop
%doc %{_docdir}/%{name}/README.md

%changelog
* Mon Jul 20 2026 yatoub <yatoub@users.noreply.github.com> - 0.1.0-1
- Initial RPM packaging for Rutile
