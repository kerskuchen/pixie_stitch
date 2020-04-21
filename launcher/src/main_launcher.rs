#![windows_subsystem = "windows"]

use ct_lib::bitmap::*;
use ct_lib::draw::*;
use ct_lib::font;
use ct_lib::font::BitmapFont;
use ct_lib::math::*;
use ct_lib::system;
use ct_lib::system::PathHelper;

use gif::SetParameter;
use indexmap::IndexMap;
use rayon::prelude::*;
use winapi;

use std::fs::File;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Constants

const TILE_SIZE: i32 = 16;
const LEGEND_BLOCK_ENTRY_COUNT: usize = 5;
const SPLIT_SEGMENT_WIDTH: i32 = 60;
const SPLIT_SEGMENT_HEIGHT: i32 = 80;
const COLOR_GRID_THIN: PixelRGBA = PixelRGBA::new(128, 128, 128, 255);
const COLOR_GRID_THICK: PixelRGBA = PixelRGBA::new(64, 64, 64, 255);

#[derive(Clone)]
struct ColorInfo {
    pub color: PixelRGBA,
    pub count: usize,
    pub symbol: Bitmap,
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Paths

fn get_executable_dir() -> String {
    if let Some(executable_path) = std::env::current_exe().ok() {
        system::path_without_filename(executable_path.to_string_borrowed())
    } else {
        ".".to_owned()
    }
}

/// Example:
/// exe path: "C:\bin\pixie_stitch.exe"
/// imagepath: "D:\images\example_image.png"
/// output_dir_suffix: "centered"
///
/// This returns:
/// "C:\bin\example_image_centered"
fn get_image_output_dir(image_filepath: &str, output_dir_suffix: &str) -> String {
    let image_filename = system::path_to_filename_without_extension(image_filepath);
    let output_dir_root = get_executable_dir();
    if output_dir_suffix.is_empty() {
        system::path_join(&output_dir_root, &image_filename)
    } else {
        system::path_join(
            &output_dir_root,
            &(image_filename + "_" + output_dir_suffix),
        )
    }
}

fn create_image_output_dir(image_filepath: &str, output_dir_suffix: &str) {
    let output_dir = get_image_output_dir(image_filepath, output_dir_suffix);
    std::fs::remove_dir_all(&output_dir).ok();
    std::fs::create_dir_all(&output_dir)
        .expect(&format!("Cannot create directory '{}'", &output_dir));
}

fn get_image_output_filepath(image_filepath: &str, output_dir_suffix: &str) -> String {
    let output_dir = get_image_output_dir(image_filepath, output_dir_suffix);
    let image_filename = system::path_to_filename_without_extension(image_filepath);
    system::path_join(&output_dir, &image_filename)
}

// NOTE: THIS IS FOR INTERNAL TESTING
#[cfg(debug_assertions)]
fn get_image_filepaths_from_commandline() -> Vec<String> {
    vec!["nathan.png".to_owned()]
    // vec!["nathan.png".to_owned(), "nathan_big.gif".to_owned()]
}

#[cfg(not(debug_assertions))]
fn get_image_filepaths_from_commandline() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();

    // NOTE: The first argument is the executable path
    args.remove(0);

    assert!(
        !args.is_empty(),
        "Please drag and drop one (or more) image(s) onto the executable"
    );

    args
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Loading resources

pub fn load_fonts() -> (BitmapFont, BitmapFont) {
    let mut font_regular = BitmapFont::new(
        font::FONT_DEFAULT_TINY_NAME,
        font::FONT_DEFAULT_TINY_TTF,
        font::FONT_DEFAULT_TINY_PIXEL_HEIGHT,
        font::FONT_DEFAULT_TINY_RASTER_OFFSET,
        0,
        0,
        PixelRGBA::black(),
        PixelRGBA::transparent(),
    );
    let mut font_big = BitmapFont::new(
        font::FONT_DEFAULT_REGULAR_NAME,
        font::FONT_DEFAULT_REGULAR_TTF,
        2 * font::FONT_DEFAULT_REGULAR_PIXEL_HEIGHT,
        font::FONT_DEFAULT_REGULAR_RASTER_OFFSET,
        0,
        0,
        PixelRGBA::black(),
        PixelRGBA::transparent(),
    );

    // NOTE: Because 0 looks like an 8 in this font on crappy printers we replace it with an O (big o)
    let regular_o = font_regular
        .glyphs
        .get(&('O' as Codepoint))
        .unwrap()
        .clone();
    let big_o = font_big.glyphs.get(&('O' as Codepoint)).unwrap().clone();
    font_regular.glyphs.insert('0' as Codepoint, regular_o);
    font_big.glyphs.insert('0' as Codepoint, big_o);

    (font_regular, font_big)
}

fn collect_symbols() -> Vec<Bitmap> {
    let executable_dir = get_executable_dir();
    let symbols_dir = {
        let candidate = system::path_join(&executable_dir, "resources");

        if system::path_exists(&candidate) {
            candidate
        } else {
            // There was no symbols dir in the executable dir. Lets try our current workingdir
            "resources".to_owned()
        }
    };

    assert!(
        system::path_exists(&symbols_dir),
        "Missing `resources` path in '{}'",
        executable_dir
    );

    let mut symbols = Vec::new();
    let symbols_filepaths = system::collect_files_by_extension_recursive(&symbols_dir, ".png");
    for symbol_filepath in &symbols_filepaths {
        let symbol_bitmap = Bitmap::create_from_png_file(symbol_filepath);
        symbols.push(symbol_bitmap);
    }
    symbols
}

fn create_alphanumeric_symbols(font: &BitmapFont) -> Vec<Bitmap> {
    let mut symbols = Vec::new();
    for c in "123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
        let mut bitmap =
            Bitmap::new_filled(TILE_SIZE as u32, TILE_SIZE as u32, PixelRGBA::transparent());
        // NOTE: We can unwrap here because we own the font and know that all glyphs exist
        let glyph_bitmap = font
            .glyphs
            .get(&(c as Codepoint))
            .as_ref()
            .unwrap()
            .bitmap
            .as_ref()
            .unwrap();
        let pos = Vec2i::new(
            block_centered_in_block(glyph_bitmap.width, TILE_SIZE),
            block_centered_in_block(glyph_bitmap.height, TILE_SIZE),
        );
        blit_symbol(glyph_bitmap, &mut bitmap, pos, PixelRGBA::transparent());
        symbols.push(bitmap);
    }

    symbols
}

fn open_image(image_filepath: &str) -> Bitmap {
    if system::path_to_extension(&image_filepath).ends_with("gif") {
        bitmap_create_from_gif_file(&image_filepath)
    } else if system::path_to_extension(&image_filepath).ends_with("png") {
        Bitmap::create_from_png_file(&image_filepath)
    } else {
        panic!("We only support GIF or PNG images");
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Low level bitmap helper function

fn blit_symbol(symbol_bitmap: &Bitmap, image: &mut Bitmap, pos: Vec2i, mask_color: PixelRGBA) {
    let symbol_rect = symbol_bitmap.rect();

    assert!(pos.x >= 0);
    assert!(pos.y >= 0);
    assert!(pos.x + symbol_rect.width() <= image.width);
    assert!(pos.y + symbol_rect.height() <= image.height);

    let dest_color = image.get(pos.x, pos.y);
    let relative_luminance = Color::from_pixelrgba(dest_color).to_relative_luminance();
    let blit_color = if relative_luminance > 0.2 {
        PixelRGBA::black()
    } else {
        PixelRGBA::white()
    };

    for y in 0..symbol_rect.height() {
        for x in 0..symbol_rect.width() {
            let symbol_pixel_color = symbol_bitmap.get(x, y);
            // NOTE: We assume the symbols-images are black on white backround. We don't want to
            //       draw the white background so we treat it as transparent
            if symbol_pixel_color != mask_color {
                image.set(pos.x + x, pos.y + y, blit_color);
            }
        }
    }
}

fn bitmap_create_from_gif_file(image_filepath: &str) -> Bitmap {
    let mut decoder = gif::Decoder::new(
        File::open(image_filepath).expect(&format!("Cannot open file '{}'", image_filepath)),
    );
    decoder.set(gif::ColorOutput::RGBA);
    let mut decoder = decoder
        .read_info()
        .expect(&format!("Cannot decode file '{}'", image_filepath));
    let frame = decoder
        .read_next_frame()
        .expect(&format!(
            "Cannot decode first frame in '{}'",
            image_filepath
        ))
        .expect(&format!("No frame found in '{}'", image_filepath));
    let buffer: Vec<PixelRGBA> = frame
        .buffer
        .chunks_exact(4)
        .into_iter()
        .map(|color| PixelRGBA::new(color[0], color[1], color[2], color[3]))
        .collect();
    Bitmap::new_from_buffer(frame.width as u32, frame.height as u32, buffer)
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Pattern creation

fn place_grid_markers_in_pattern(
    bitmap: &Bitmap,
    font: &BitmapFont,
    grid_size: i32,
    place_marker_every: i32,
    marker_start_x: i32,
    marker_start_y: i32,
) -> Bitmap {
    // Determine how many markers we need
    let (marker_count_x, marker_count_y) = {
        let grid_count_x = bitmap.width / grid_size;
        let grid_count_y = bitmap.height / grid_size;
        (
            grid_count_x / place_marker_every,
            grid_count_y / place_marker_every,
        )
    };

    // Determine how much padding we need
    let marker_padding = {
        let marker_max_x = marker_start_x + marker_count_x * place_marker_every;
        let marker_max_y = marker_start_y + marker_count_y * place_marker_every;
        let marker_max = i32::max(marker_max_x, marker_max_y);
        let text_dim = font
            .get_text_bounding_rect(&format!("  {}  ", marker_max), 1)
            .dim;
        i32::max(text_dim.x, text_dim.y)
    };

    let mut result_bitmap = bitmap.extended(
        marker_padding,
        marker_padding,
        marker_padding,
        marker_padding,
        PixelRGBA::white(),
    );

    // Add x markers
    for marker_index in 0..=marker_count_x {
        let text = (marker_start_x + marker_index * place_marker_every).to_string();
        let x = marker_padding + marker_index * grid_size * place_marker_every;
        let pos_top = Vec2i::new(x, marker_padding / 2);
        let pos_bottom = Vec2i::new(x, result_bitmap.height - marker_padding / 2);

        result_bitmap.draw_text_aligned_in_point_exact(
            font,
            &text,
            1,
            pos_top,
            Vec2i::zero(),
            false,
            AlignmentHorizontal::Center,
            AlignmentVertical::Center,
        );
        result_bitmap.draw_text_aligned_in_point_exact(
            font,
            &text,
            1,
            pos_bottom,
            Vec2i::zero(),
            false,
            AlignmentHorizontal::Center,
            AlignmentVertical::Center,
        );
    }

    // Add y markers
    for marker_index in 0..=marker_count_y {
        let text = (marker_start_y + marker_index * place_marker_every).to_string();
        let y = marker_padding + marker_index * grid_size * place_marker_every;
        let pos_left = Vec2i::new(marker_padding / 2, y);
        let pos_right = Vec2i::new(result_bitmap.width - marker_padding / 2, y);

        result_bitmap.draw_text_aligned_in_point_exact(
            font,
            &text,
            1,
            pos_left,
            Vec2i::zero(),
            false,
            AlignmentHorizontal::Center,
            AlignmentVertical::Center,
        );
        result_bitmap.draw_text_aligned_in_point_exact(
            font,
            &text,
            1,
            pos_right,
            Vec2i::zero(),
            false,
            AlignmentHorizontal::Center,
            AlignmentVertical::Center,
        );
    }

    result_bitmap
}

fn create_cross_stitch_pattern(
    bitmap: &Bitmap,
    font_grid_marker: &BitmapFont,
    font_segment_index_indicator: &BitmapFont,
    image_filepath: &str,
    output_filename_suffix: &str,
    output_dir_suffix: &str,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
    segment_index: Option<usize>,
    marker_start_x: i32,
    marker_start_y: i32,
    colorize: bool,
    add_symbol: bool,
    add_thick_ten_grid: bool,
    symbol_mask_color: PixelRGBA,
) {
    // NOTE: To add closing lines of the thick and thin grid lines we need to leave some padding on
    //       the right and bottom of the image
    let padding_right = 1 + if bitmap.width % 10 == 0 && add_thick_ten_grid {
        1
    } else {
        0
    };
    let padding_bottom = 1 + if bitmap.height % 10 == 0 && add_thick_ten_grid {
        1
    } else {
        0
    };
    let mut scaled_bitmap = Bitmap::new(
        padding_right + (TILE_SIZE * bitmap.width) as u32,
        (padding_bottom + TILE_SIZE * bitmap.height) as u32,
    );

    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let color = bitmap.get(x, y);

            // Colorize pixels
            if colorize {
                scaled_bitmap.draw_rect_filled(
                    TILE_SIZE * x,
                    TILE_SIZE * y,
                    TILE_SIZE,
                    TILE_SIZE,
                    if color.a == 0 {
                        PixelRGBA::white()
                    } else {
                        color
                    },
                );
            } else {
                scaled_bitmap.draw_rect_filled(
                    TILE_SIZE * x,
                    TILE_SIZE * y,
                    TILE_SIZE,
                    TILE_SIZE,
                    PixelRGBA::white(),
                );
            }

            // Add symbol
            if add_symbol && color.a != 0 {
                let symbol = &color_mappings.get(&color).unwrap().symbol;
                blit_symbol(
                    symbol,
                    &mut scaled_bitmap,
                    Vec2i::new(TILE_SIZE * x, TILE_SIZE * y),
                    symbol_mask_color,
                );
            }

            // Add 1x1 grid
            scaled_bitmap.draw_rect_filled(
                TILE_SIZE * x,
                TILE_SIZE * y,
                1,
                TILE_SIZE,
                COLOR_GRID_THIN,
            );
            scaled_bitmap.draw_rect_filled(
                TILE_SIZE * x,
                TILE_SIZE * y,
                TILE_SIZE,
                1,
                COLOR_GRID_THIN,
            );

            // Add 10x10 grid
            if add_thick_ten_grid {
                if x % 10 == 0 {
                    scaled_bitmap.draw_rect_filled(
                        TILE_SIZE * x,
                        TILE_SIZE * y,
                        2,
                        TILE_SIZE,
                        COLOR_GRID_THICK,
                    );
                }
                if y % 10 == 0 {
                    scaled_bitmap.draw_rect_filled(
                        TILE_SIZE * x,
                        TILE_SIZE * y,
                        TILE_SIZE,
                        2,
                        COLOR_GRID_THICK,
                    );
                }
            }
        }
    }

    // Close 1x1 line on right bitmap border
    scaled_bitmap.draw_rect_filled(
        TILE_SIZE * bitmap.width,
        0,
        1,
        TILE_SIZE * bitmap.height,
        COLOR_GRID_THIN,
    );
    // Close 1x1 line on bottom bitmap border
    scaled_bitmap.draw_rect_filled(
        0,
        TILE_SIZE * bitmap.height,
        TILE_SIZE * bitmap.width,
        1,
        COLOR_GRID_THIN,
    );

    if add_thick_ten_grid {
        // Close 10x10 line on right bitmap border
        if bitmap.width % 10 == 0 {
            scaled_bitmap.draw_rect_filled(
                TILE_SIZE * bitmap.width,
                0,
                2,
                TILE_SIZE * bitmap.height,
                COLOR_GRID_THICK,
            );
        }
        // Close 10x10 line on bottom bitmap border
        if bitmap.height % 10 == 0 {
            scaled_bitmap.draw_rect_filled(
                0,
                TILE_SIZE * bitmap.height,
                TILE_SIZE * bitmap.width,
                2,
                COLOR_GRID_THICK,
            );
        }
    }

    // Add 10-grid markers
    let final_bitmap = if add_thick_ten_grid {
        place_grid_markers_in_pattern(
            &scaled_bitmap,
            font_grid_marker,
            TILE_SIZE,
            10,
            marker_start_x,
            marker_start_y,
        )
    } else {
        scaled_bitmap
    };

    // Add segment index indicator if necessary
    let final_bitmap = if let Some(segment_index) = segment_index {
        let text_bitmap = Bitmap::create_from_text(
            font_segment_index_indicator,
            &format!("\n Pattern Part {} \n", segment_index),
            1,
            PixelRGBA::white(),
        );
        text_bitmap.glued_to(
            &final_bitmap,
            GluePosition::TopCenter,
            0,
            PixelRGBA::white(),
        )
    } else {
        final_bitmap
    };

    // Write out png image
    let output_filepath = get_image_output_filepath(&image_filepath, output_dir_suffix)
        + "_"
        + output_filename_suffix
        + ".png";
    Bitmap::write_to_png_file(&final_bitmap, &output_filepath);
}

fn create_cross_stitch_pattern_set(
    image: &Bitmap,
    font_grid_marker: &BitmapFont,
    font_segment_index_indicator: &BitmapFont,
    image_filepath: &str,
    output_filename_suffix: &str,
    output_dir_suffix: &str,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
    color_mappings_alphanum: &IndexMap<PixelRGBA, ColorInfo>,
    segment_index: Option<usize>,
    marker_start_x: i32,
    marker_start_y: i32,
    create_paint_by_number_set: bool,
) {
    rayon::scope(|scope| {
        scope.spawn(|_| {
            create_cross_stitch_pattern(
                &image,
                font_grid_marker,
                font_segment_index_indicator,
                &image_filepath,
                &("cross_stitch_colorized_".to_owned() + output_filename_suffix),
                output_dir_suffix,
                &color_mappings,
                segment_index,
                marker_start_x,
                marker_start_y,
                true,
                true,
                true,
                PixelRGBA::white(),
            );
        });
        scope.spawn(|_| {
            create_cross_stitch_pattern(
                &image,
                font_grid_marker,
                font_segment_index_indicator,
                &image_filepath,
                &("cross_stitch_".to_owned() + output_filename_suffix),
                output_dir_suffix,
                &color_mappings,
                segment_index,
                marker_start_x,
                marker_start_y,
                false,
                true,
                true,
                PixelRGBA::white(),
            );
        });
        scope.spawn(|_| {
            create_cross_stitch_pattern(
                &image,
                font_grid_marker,
                font_segment_index_indicator,
                &image_filepath,
                &("cross_stitch_colorized_no_symbols_".to_owned() + output_filename_suffix),
                output_dir_suffix,
                &color_mappings,
                segment_index,
                marker_start_x,
                marker_start_y,
                true,
                false,
                true,
                PixelRGBA::white(),
            );
        });
        if create_paint_by_number_set {
            scope.spawn(|_| {
                create_cross_stitch_pattern(
                    &image,
                    font_grid_marker,
                    font_segment_index_indicator,
                    &image_filepath,
                    &("paint_by_numbers_".to_owned() + output_filename_suffix),
                    output_dir_suffix,
                    &color_mappings_alphanum,
                    segment_index,
                    marker_start_x,
                    marker_start_y,
                    false,
                    true,
                    false,
                    PixelRGBA::transparent(),
                );
            });
        }
    });
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Pattern creation

fn image_extract_colors_and_counts(image: &Bitmap) -> IndexMap<PixelRGBA, ColorInfo> {
    let mut color_mappings = IndexMap::new();
    for pixel in &image.data {
        if pixel.a == 0 {
            // Ignore transparent regions
            continue;
        }

        let entry = color_mappings.entry(*pixel).or_insert_with(|| ColorInfo {
            color: *pixel,
            count: 0,
            symbol: Bitmap::empty(),
        });
        entry.count += 1;
    }

    // This makes color ramps on the legend more pretty
    color_mappings.sort_by(|color_a, _info_a, color_b, _info_b| {
        PixelRGBA::compare_by_hue_luminosity_saturation(color_a, color_b)
    });

    color_mappings
}

fn image_map_colors_to_symbols(
    color_mappings: &mut IndexMap<PixelRGBA, ColorInfo>,
    symbols: &[Bitmap],
) {
    assert!(color_mappings.len() <= symbols.len());
    for (entry, symbol) in color_mappings.values_mut().zip(symbols.iter()) {
        entry.symbol = symbol.clone();
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Legend creation

fn create_pattern_page_layout(font: &BitmapFont, layout_indices: &[Vec2i]) -> Bitmap {
    let caption_image =
        Bitmap::create_from_text(font, "\n\nPattern parts overview:\n", 1, PixelRGBA::white());

    let page_count = layout_indices.len();
    // NOTE: Indexes begin at 0 therefore we add 1
    let num_rows = 1 + layout_indices.iter().map(|v| v.y).max().unwrap();
    let num_columns = 1 + layout_indices.iter().map(|v| v.x).max().unwrap();

    let page_tile_dim = {
        // NOTE: We want to have a 1px visual gap between page tiles therefore we add 1
        let page_tile_width = 1 + font
            .get_text_bounding_rect(&format!(" {} ", page_count), 1)
            .dim
            .x;
        let page_tile_height = 1 + (page_tile_width as f32 * (9.0 / 6.0)) as i32;
        Vec2i::new(page_tile_width, page_tile_height)
    };

    let image_width = num_columns * page_tile_dim.x;
    let image_height = num_rows * page_tile_dim.y;
    let mut image = Bitmap::new_filled(image_width as u32, image_height as u32, PixelRGBA::white());
    for (page_index, pos_index) in layout_indices.iter().enumerate() {
        let pos = *pos_index * page_tile_dim;
        image.draw_rect(
            pos.x,
            pos.y,
            page_tile_dim.x - 1,
            page_tile_dim.y - 1,
            PixelRGBA::black(),
        );
        image.draw_text_aligned_in_point(
            font,
            &(page_index + 1).to_string(),
            1,
            pos + page_tile_dim / 2,
            Vec2i::zero(),
            false,
            AlignmentHorizontal::Center,
            AlignmentVertical::Center,
        );
    }

    caption_image.glued_to(&image, GluePosition::TopLeft, 0, PixelRGBA::white())
}

fn create_legend_entry(font: &BitmapFont, info: &ColorInfo) -> Bitmap {
    // Draw color and symbol mapping
    let mut color_symbol_map =
        Bitmap::new_filled(2 * TILE_SIZE as u32, TILE_SIZE as u32, PixelRGBA::white());
    color_symbol_map.draw_rect_filled(0, 0, TILE_SIZE, TILE_SIZE, info.color);
    color_symbol_map.draw_rect(
        0,
        0,
        TILE_SIZE,
        TILE_SIZE,
        PixelRGBA::from_color(Color::black()),
    );
    blit_symbol(
        &info.symbol,
        &mut color_symbol_map,
        Vec2i::filled_x(TILE_SIZE),
        PixelRGBA::white(),
    );
    color_symbol_map.draw_rect(
        0 + TILE_SIZE,
        0,
        TILE_SIZE,
        TILE_SIZE,
        PixelRGBA::from_color(Color::black()),
    );

    // Add stitches info
    let stitches_info = Bitmap::create_from_text(
        font,
        &format!(" {} stitches      ", info.count),
        1,
        PixelRGBA::white(),
    );
    stitches_info.glued_to(
        &mut color_symbol_map,
        GluePosition::RightCenter,
        0,
        PixelRGBA::white(),
    )
}

fn create_legend_block(font: &BitmapFont, infos: &[ColorInfo]) -> Bitmap {
    let entries: Vec<Bitmap> = infos
        .iter()
        .map(|entry| create_legend_entry(font, entry))
        .collect();
    Bitmap::glue_together_multiple(
        &entries,
        GluePosition::BottomLeft,
        TILE_SIZE,
        PixelRGBA::white(),
    )
}

fn create_cross_stitch_legend(
    image_dimensions: Vec2i,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
    image_filepath: &str,
    output_dir_suffix: &str,
    font: &BitmapFont,
    segment_layout_indices: &[Vec2i],
) {
    let mut legend = {
        // Create color and stitch stats
        let stats_bitmap = {
            let color_count = color_mappings.len();
            let stitch_count = color_mappings
                .values()
                .fold(0, |acc, entry| acc + entry.count);

            Bitmap::create_from_text(
                font,
                &format!(
                    "Size:     {}x{}\n\nColors:   {}\n\nStitches: {}\n\n\n",
                    image_dimensions.x, image_dimensions.y, color_count, stitch_count
                ),
                1,
                PixelRGBA::white(),
            )
        };

        // Create color mapping blocks
        let blocks = {
            let color_infos: Vec<ColorInfo> = color_mappings.values().cloned().collect();
            let block_bitmaps: Vec<Bitmap> = color_infos
                .chunks(LEGEND_BLOCK_ENTRY_COUNT)
                .map(|chunk| create_legend_block(font, chunk))
                .collect();
            let num_columns = block_bitmaps.len().max(4);
            let block_rows: Vec<Bitmap> = block_bitmaps
                .chunks(num_columns)
                .map(|chunk| {
                    Bitmap::glue_together_multiple(
                        chunk,
                        GluePosition::RightTop,
                        TILE_SIZE,
                        PixelRGBA::white(),
                    )
                })
                .collect();
            Bitmap::glue_together_multiple(
                &block_rows,
                GluePosition::BottomLeft,
                TILE_SIZE,
                PixelRGBA::white(),
            )
            .extended(0, 0, 0, (1.5 * TILE_SIZE as f32) as i32, PixelRGBA::white())
        };

        Bitmap::glue_a_to_b(
            &stats_bitmap,
            &blocks,
            GluePosition::TopLeft,
            0,
            PixelRGBA::white(),
        )
    };

    // Add page layout order if necessary
    if segment_layout_indices.len() > 1 {
        let page_layout_image = create_pattern_page_layout(font, segment_layout_indices);

        legend = legend.glued_to(
            &page_layout_image,
            GluePosition::TopLeft,
            0,
            PixelRGBA::white(),
        );

        // Draw separating line between colors and page order layout
        for x in 0..legend.width {
            legend.set(
                x,
                legend.height - page_layout_image.height,
                PixelRGBA::black(),
            );
        }
    }

    let padding = TILE_SIZE;
    let final_image = legend.extended(padding, padding, padding, padding, PixelRGBA::white());

    // Write out png image
    let output_filepath =
        get_image_output_filepath(&image_filepath, output_dir_suffix) + "_legend.png";
    Bitmap::write_to_png_file(&final_image, &output_filepath);
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Main

#[cfg(windows)]
fn show_messagebox(caption: &str, message: &str, is_error: bool) {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::winuser::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK};

    let caption_wide: Vec<u16> = std::ffi::OsStr::new(caption)
        .encode_wide()
        .chain(once(0))
        .collect();
    let message_wide: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(once(0))
        .collect();

    unsafe {
        MessageBoxW(
            null_mut(),
            message_wide.as_ptr(),
            caption_wide.as_ptr(),
            MB_OK
                | if is_error {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                },
        )
    };
}

fn set_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let (message, location) = ct_lib::panic_message_split_to_message_and_location(panic_info);
        let final_message = format!("{}\n\nError occured at: {}", message, location);

        show_messagebox("Pixel Stitch Error", &final_message, true);

        // NOTE: This forces the other threads to shutdown as well
        std::process::abort();
    }));
}

fn main() {
    set_panic_hook();

    // NOTE: We can uncomment this if we want to test our color sorting and symbol contrast
    // test_color_sorting();
    // test_symbols_contrast();

    let (font, font_big) = load_fonts();
    let symbols = collect_symbols();
    let symbols_alphanum = create_alphanumeric_symbols(&font);

    // NOTE: We can uncomment this if we want to test with more colors than we have symbols
    /*
    symbols = symbols.iter().cloned().cycle().take(50000).collect();
    symbols_alphanum = symbols_alphanum
        .iter()
        .cloned()
        .cycle()
        .take(50000)
        .collect();
    */

    for image_filepath in get_image_filepaths_from_commandline() {
        create_image_output_dir(&image_filepath, "");
        create_image_output_dir(&image_filepath, "centered");

        let image = open_image(&image_filepath);

        let mut color_mappings = image_extract_colors_and_counts(&image);
        let mut color_mappings_alphanum = color_mappings.clone();

        assert!(
            symbols.len() >= color_mappings.len(),
            "Not enough symbols to map {} colors found in given image '{}' for cross stitch",
            color_mappings.len(),
            &image_filepath,
        );
        assert!(
            symbols_alphanum.len() >= color_mappings_alphanum.len(),
            "Not enough symbols to map {} colors found in given image '{}' for paint by numbers",
            color_mappings.len(),
            &image_filepath,
        );

        image_map_colors_to_symbols(&mut color_mappings, &symbols);
        image_map_colors_to_symbols(&mut color_mappings_alphanum, &symbols_alphanum);

        let (segment_images, segment_coordinates) =
            image.to_segments(SPLIT_SEGMENT_WIDTH, SPLIT_SEGMENT_HEIGHT);

        rayon::scope(|scope| {
            // Legend
            scope.spawn(|_| {
                create_cross_stitch_legend(
                    image.dim(),
                    &color_mappings,
                    &image_filepath,
                    "",
                    &font,
                    &segment_coordinates,
                );
            });
            scope.spawn(|_| {
                create_cross_stitch_legend(
                    image.dim(),
                    &color_mappings,
                    &image_filepath,
                    "centered",
                    &font,
                    &segment_coordinates,
                );
            });

            // Create patterns for complete set
            create_cross_stitch_pattern_set(
                &image,
                &font,
                &font_big,
                &image_filepath,
                "complete",
                "",
                &color_mappings,
                &color_mappings_alphanum,
                None,
                0,
                0,
                true,
            );
            create_cross_stitch_pattern_set(
                &image,
                &font,
                &font_big,
                &image_filepath,
                "complete",
                "centered",
                &color_mappings,
                &color_mappings_alphanum,
                None,
                0,
                0,
                true,
            );

            // Create patterns for individual segments if needed
            if segment_images.len() > 1 {
                segment_images
                    .par_iter()
                    .zip(segment_coordinates.par_iter())
                    .enumerate()
                    .for_each(|(segment_index, (segment_image, segment_coordinate))| {
                        let marker_start_x = SPLIT_SEGMENT_WIDTH * segment_coordinate.x;
                        let marker_start_y = SPLIT_SEGMENT_HEIGHT * segment_coordinate.y;

                        create_cross_stitch_pattern_set(
                            segment_image,
                            &font,
                            &font_big,
                            &image_filepath,
                            &format!("segment_{}", segment_index + 1),
                            "",
                            &color_mappings,
                            &color_mappings_alphanum,
                            Some(segment_index + 1),
                            marker_start_x,
                            marker_start_y,
                            false,
                        );
                        create_cross_stitch_pattern_set(
                            segment_image,
                            &font,
                            &font_big,
                            &image_filepath,
                            &format!("segment_{}", segment_index + 1),
                            "centered",
                            &color_mappings,
                            &color_mappings_alphanum,
                            Some(segment_index + 1),
                            marker_start_x,
                            marker_start_y,
                            false,
                        );
                    });
            }
        });
    }

    show_messagebox("Pixel Stitch", "Finished creating patterns. Enjoy!", false);
}

////////////////////////////////////////////////////////////////////////////////////////////////////
/// Test functions

/// This is for test purposes. It creates a colorful image and repeats all available symbols onto
/// it. Its main pupose was testing wheter the algorithm chooses black or white symbols correctly
/// depending on the relative luminance of the background color
#[allow(dead_code)]
fn test_symbols_contrast() {
    let (font, font_big) = load_fonts();

    fn create_test_color_ramp_bitmap() -> Bitmap {
        let mut colors = Vec::new();

        for luminance_step in 0..32 {
            let luminance = luminance_step as f64 / 10.0;
            for hue_step in 0..32 {
                let hue = 360.0 * (hue_step as f64 / 10.0);
                for saturation_step in 0..16 {
                    let saturation = saturation_step as f64 / 10.0;

                    let color = hsl::HSL {
                        h: hue,
                        s: saturation,
                        l: luminance,
                    };

                    let color_rgb = color.to_rgb();
                    colors.push(PixelRGBA::new(color_rgb.0, color_rgb.1, color_rgb.2, 255));
                }
            }
        }

        // NOTE: sqrt(luminace_steps * hue_steps * saturation_steps) = sqrt(32 * 32 * 16) = 128
        let mut result = Bitmap::empty();
        result.width = 128;
        result.height = 128;
        result.data = colors;

        result
    }

    let image = create_test_color_ramp_bitmap();
    Bitmap::write_to_png_file(&image, "test_symbol_contrast.png");

    let mut color_mappings = image_extract_colors_and_counts(&image);
    let mut symbols = collect_symbols();

    while symbols.len() < color_mappings.len() {
        symbols = [&symbols[..], &symbols[..]].concat()
    }

    image_map_colors_to_symbols(&mut color_mappings, &mut symbols);
    create_cross_stitch_pattern(
        &image,
        &font,
        &font_big,
        "test_symbol_contrast.png",
        "cross_stitch_colorized",
        "",
        &color_mappings,
        None,
        0,
        0,
        true,
        true,
        true,
        PixelRGBA::white(),
    );
}

/// This is for test purposes. It creates a colorful image and sorts its colors to test the
/// perceptive quality of the sorting
#[allow(dead_code)]
fn test_color_sorting() {
    fn create_test_color_ramp_bitmap() -> Bitmap {
        let mut colors = Vec::new();

        for red in (0..=255).step_by(4) {
            for green in (0..=255).step_by(4) {
                for blue in (0..=255).step_by(4) {
                    colors.push(PixelRGBA::new(red, green, blue, 255));
                }
            }
        }

        // NOTE: sqrt(red_steps * green_steps * blue_steps) = sqrt(64 * 64 * 64) = 512
        let mut result = Bitmap::empty();
        result.width = 512;
        result.height = 512;
        result.data = colors;

        result
    }

    let mut image = create_test_color_ramp_bitmap();
    Bitmap::write_to_png_file(&image, "test_all_colors.png");

    image
        .data
        .sort_by(|a, b| PixelRGBA::compare_by_hue_luminosity_saturation(a, b));

    Bitmap::write_to_png_file(&image, "test_all_colors_sorted.png");
}
