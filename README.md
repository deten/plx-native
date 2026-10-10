# PlxNative: deten's fork

A personal fork of [PlxNative](https://github.com/GLinnik21/plx-native), the native Plex client
for LG webOS televisions. This page covers what the fork adds. The
[upstream README](https://github.com/GLinnik21/plx-native#readme) and
[plxnative.com](https://plxnative.com/) cover everything else: features, supported televisions,
the official install and troubleshooting.

This is an independent fork. Test builds from it are unofficial, so please report problems with
them here rather than upstream.

## What this fork adds

### Library order (in testing)

You choose one order for your libraries, per profile, and the app uses it in two places:

- **Home** starts with Continue Watching, then shows each library's Recently Added shelf in your
  order, then recommendations and the rest.
- **The Movies and TV Shows tabs** list their library pills in the same order.

Set it in **Settings > Libraries > Library order**. Select a library to pick it up, move it with
Up and Down, then select it again to put it down. Each move saves right away.

| Library order | Moving a library |
|---|---|
| ![Library order page](docs/fork/library-order.jpg) | ![Kids Movies picked up](docs/fork/library-order-moving.jpg) |

| Home after the move | Movies tab after the move |
|---|---|
| ![Home with Kids Movies first](docs/fork/home-after-reorder.jpg) | ![Kids Movies is the first pill](docs/fork/movies-tab-after-reorder.jpg) |

*The screenshots come from the project's simulator and its openly licensed demo films.*

Good to know:
- Home reorders the Recently Added shelves your server sends. A library your server leaves off its
  home screen stays off Home.
- Shelf headings use the server's wording, such as "Recently Added in Kids Movies".
- Movies and shows share one list.

Code: branch [`library-order`](../../tree/library-order).

### Subtitle auto-sync (in progress)

Plex Media Server can re-time a downloaded subtitle to match the video, and its subtitle search
marks which candidates support it (`canAutoSync`). This fork adds a way to ask for that from the
player's subtitle menu.

## Try a test build

Test builds are published as [pre-releases](../../releases). They use the **debug** flavour, which
installs as its own app, **PlxNative debug** (`com.beb.plxnative.debug`), with its own launcher
tile and sign-in. It sits alongside your regular PlxNative and the official Plex app.

On a rooted television with Homebrew Channel, install the `.ipk` from a release. To remove it,
uninstall **PlxNative debug** like any other app.

The debug flavour keeps upstream's developer checks switched on, so it can be stricter than the
regular app.

## Building

Builds use upstream's own pipeline. The `CI` workflow's cross-build job makes the debug `.ipk` on
GitHub's ARM runners; run it from the Actions tab on the branch you want. Local builds need a Mac
or an arm64 Linux machine, as described in the [install guide](docs/install-and-verify.md).

## License

GPL-3.0, the same as upstream. See [LICENSE](LICENSE) and [LICENSING.md](LICENSING.md).
