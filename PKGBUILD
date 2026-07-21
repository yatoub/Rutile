pkgname=rutile
pkgver=0.2.0
pkgrel=1
pkgdesc='GNOME-native terminal emulator with split tiling and synchronized input'
url='https://github.com/yatoub/Rutile'
license=('MIT')
makedepends=('cargo')
depends=('gtk4' 'libadwaita' 'vte4')
arch=('x86_64' 'aarch64')
source=("https://github.com/yatoub/Rutile/archive/refs/tags/v$pkgver.tar.gz")
b2sums=(4ade86fcc13d3058e390ac508b14fe3a6c45f10142878dc918292909059b1c37df9a167853b1df9edec595e047387263cd3e60c70875877c9379cc5e889b84d9)

prepare() {
    cd Rutile-$pkgver
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed 's/host: //;t;d')"
}

build() {
    cd Rutile-$pkgver
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

check() {
    cd Rutile-$pkgver
    export RUSTUP_TOOLCHAIN=stable
    cargo test --frozen
}

package() {
    cd Rutile-$pkgver
    install -Dm0755 -t "$pkgdir/usr/bin/" "target/release/$pkgname"
    install -Dm0644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    install -Dm0644 resources/rutile.desktop "$pkgdir/usr/share/applications/rutile.desktop"
    install -Dm0644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
}
