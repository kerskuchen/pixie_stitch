# Pixie Stitch

A cross-stitch (and fusible-beads) pattern generator for Windows that is specialized for pixel art
and ease of use via drag-and-dropping of images. 

# Usage

The easiest way to use Pixie Stitch is to download the latest release from the 
[release page](https://github.com/kerskuchen/pixie_stitch/releases) and extract it to a directory 
of our choice. 

We then can start drag-and-dropping our pixel-art images onto the `pixie_stitch.exe`.
Doing that will create a new folder which contains our cross-stitch-patterns and is named like 
our image.

![Example conversion output](example.png "Example conversion output")

Additional example output can be found in the `examples` directory.

# Limitations

As of now Pixie Stitch supports `.png` and `.gif` images with up to 17 colors. The number of colors
can be increased by adding additional black-and-white `16x16`-pixels-sized symbol images in the 
`resources` folder where our executable is located.

# Building it

Assuming we have [Rust](https://www.rust-lang.org/) installed and can run `cargo` commands we can
build a release version by just running `windows_build_shipping.bat`. This creates a new folder 
named `windows_shipping` which contains the final executable ready to run with all needed
resources.

If we have the [Resource Hacker](http://angusj.com/resourcehacker/) tool in our `%PATH` the 
`windows_build_shipping.bat` will also set a launcher icon and version information for our 
executable.

We can also build a debug version by running the usual `cargo build` command. The 
[Rust](https://www.rust-lang.org/) website has good information about how to start development 
with Rust.

# Similar software

There is lot of existing software for cross-stitch pattern generation like:

* https://www.pixel-stitch.net/
* https://www.stitchfiddle.com/en
* http://www.myphotostitch.com/Make_Pattern/Make_Pattern.html
* https://www.pic2pat.com/index.en.php

These have lot of extra feature that Pixie Stitch does not have like size and color palette 
interpolation. Pixie stitch has a very narrow use-case and is specialized for small pixel art with
few colors and optional transparency.

