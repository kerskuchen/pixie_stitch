#![windows_subsystem = "windows"]

use cottontail::core::*;
use cottontail::image::{bitmap::*, color::hsl, font::*};
use cottontail::math::*;
use cottontail::{core::PathHelper, image::ColorBlendMode};

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

enum PatternType {
    BlackAndWhite,
    Colorized,
    ColorizedNoSymbols,
    PaintByNumbers,
}

struct Resources {
    font: BitmapFont,
    font_big: BitmapFont,
    stitch_background_image_8x8_premultiplied_alpha: Bitmap,
}

#[derive(Clone)]
struct ColorInfo {
    pub color: PixelRGBA,
    pub count: usize,
    pub symbol: Bitmap,
    pub symbol_alphanum: Bitmap,
    pub stitches_premultiplied: Vec<Bitmap>,
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Paths

fn get_executable_dir() -> String {
    if let Some(executable_path) = std::env::current_exe().ok() {
        path_without_filename(executable_path.to_string_borrowed_or_panic())
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
    let image_filename = path_to_filename_without_extension(image_filepath);
    let output_dir_root = get_executable_dir();
    if output_dir_suffix.is_empty() {
        path_join(&output_dir_root, &image_filename)
    } else {
        path_join(
            &output_dir_root,
            &(image_filename + "_" + output_dir_suffix),
        )
    }
}

// NOTE: This is for quicker testing to keep images open in imageviewer
#[cfg(debug_assertions)]
fn create_image_output_dir(image_filepath: &str, output_dir_suffix: &str) {
    let output_dir = get_image_output_dir(image_filepath, output_dir_suffix);
    if !path_exists(&output_dir) {
        std::fs::create_dir_all(&output_dir)
            .expect(&format!("Cannot create directory '{}'", &output_dir));
    }
}

#[cfg(not(debug_assertions))]
fn create_image_output_dir(image_filepath: &str, output_dir_suffix: &str) {
    let output_dir = get_image_output_dir(image_filepath, output_dir_suffix);
    if path_exists(&output_dir) {
        std::fs::remove_dir_all(&output_dir).expect(&format!(
            "Cannot overwrite directory '{}': is a file from it still open?",
            &output_dir
        ));
    }
    std::fs::create_dir_all(&output_dir)
        .expect(&format!("Cannot create directory '{}'", &output_dir));
}

fn get_image_output_filepath(image_filepath: &str, output_dir_suffix: &str) -> String {
    let output_dir = get_image_output_dir(image_filepath, output_dir_suffix);
    let image_filename = path_to_filename_without_extension(image_filepath);
    path_join(&output_dir, &image_filename)
}

// NOTE: THIS IS FOR INTERNAL TESTING
#[cfg(debug_assertions)]
fn get_image_filepaths_from_commandline() -> Vec<String> {
    vec![
        "examples/nathan.png".to_owned(),
        "examples/nathan_big.gif".to_owned(),
        "examples/pixie.png".to_owned(),
        // "nachtlicht-pixel2.gif".to_owned(),
        // "nachtlicht-pixel2wolke.png".to_owned(),
    ]
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

fn get_resource_dir_path() -> String {
    let executable_dir_path = get_executable_dir();
    let resource_dir_path = {
        let candidate = path_join(&executable_dir_path, "resources");

        if path_exists(&candidate) {
            candidate
        } else {
            // There was no symbols dir in the executable dir. Lets try our current workingdir
            "resources".to_owned()
        }
    };

    assert!(
        path_exists(&resource_dir_path),
        "Missing `resources` path in '{}'",
        executable_dir_path
    );

    resource_dir_path
}

fn load_stitch_preview_images_premultiplied_alpha() -> (Vec<Bitmap>, Vec<Bitmap>, Bitmap) {
    let resource_dir_path = get_resource_dir_path();
    let background_tile_image_8x8 =
        Bitmap::from_png_file_or_panic(&path_join(&resource_dir_path, "aida_8x8.png"))
            .to_premultiplied_alpha();
    let stitch_tile_images = ["stitch1.png", "stitch2.png", "stitch3.png"]
        .iter()
        .map(|filename| {
            Bitmap::from_png_file_or_panic(&path_join(&resource_dir_path, filename))
                .to_premultiplied_alpha()
        })
        .collect();
    let stitch_tile_images_luminance = ["stitch1_lum.png", "stitch2_lum.png", "stitch3_lum.png"]
        .iter()
        .map(|filename| {
            Bitmap::from_png_file_or_panic(&path_join(&resource_dir_path, filename))
                .to_premultiplied_alpha()
        })
        .collect();
    (
        stitch_tile_images,
        stitch_tile_images_luminance,
        background_tile_image_8x8,
    )
}

pub fn load_fonts() -> (BitmapFont, BitmapFont) {
    let mut font_regular = BitmapFont::new(
        FONT_DEFAULT_TINY_NAME,
        FONT_DEFAULT_TINY_TTF,
        FONT_DEFAULT_TINY_PIXEL_HEIGHT,
        FONT_DEFAULT_TINY_RASTER_OFFSET,
        0,
        0,
        PixelRGBA::black(),
        PixelRGBA::transparent(),
    );
    let mut font_big = BitmapFont::new(
        FONT_DEFAULT_REGULAR_NAME,
        FONT_DEFAULT_REGULAR_TTF,
        2 * FONT_DEFAULT_REGULAR_PIXEL_HEIGHT,
        FONT_DEFAULT_REGULAR_RASTER_OFFSET,
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
    let resource_dir_path = get_resource_dir_path();
    let symbols_filepaths = collect_files_by_extension_recursive(&resource_dir_path, ".png");
    symbols_filepaths
        .into_iter()
        .filter(|filepath| {
            path_to_filename_without_extension(filepath)
                .parse::<u32>()
                .is_ok()
        })
        .map(|symbol_filepath| Bitmap::from_png_file_or_panic(&symbol_filepath))
        .collect()
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
    if path_to_extension(&image_filepath).ends_with("gif") {
        bitmap_create_from_gif_file(&image_filepath)
    } else if path_to_extension(&image_filepath).ends_with("png") {
        Bitmap::from_png_file_or_panic(&image_filepath)
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

fn draw_origin_line_vertical(bitmap: &mut Bitmap, pos_x: i32) {
    bitmap.draw_rect_filled_safely(pos_x - 2, 0, 4, bitmap.height, PixelRGBA::black());
    bitmap.draw_rect_filled_safely(pos_x - 1, 0, 2, bitmap.height, PixelRGBA::white());
}

fn draw_origin_line_horizontal(bitmap: &mut Bitmap, pos_y: i32) {
    bitmap.draw_rect_filled_safely(0, pos_y - 2, bitmap.width, 4, PixelRGBA::black());
    bitmap.draw_rect_filled_safely(0, pos_y - 1, bitmap.width, 2, PixelRGBA::white());
}

/// NOTE: This assumes that the scaled bitmap width and height are a roughly a multiple of
///       grid_cell_size
fn place_grid_labels_in_pattern(
    scaled_bitmap: &Bitmap,
    grid_cell_size: i32,
    font: &BitmapFont,
    logical_first_coordinate_x: i32,
    logical_first_coordinate_y: i32,
) -> Bitmap {
    let grid_width = scaled_bitmap.width / grid_cell_size;
    let grid_height = scaled_bitmap.height / grid_cell_size;

    let logical_last_coordinate_x = logical_first_coordinate_x + grid_width;
    let logical_last_coordinate_y = logical_first_coordinate_y + grid_height;

    // Determine how much image-padding we need by calculating the maximum label text dimension
    let label_padding = {
        let max_logical_coordinates = [
            logical_first_coordinate_x,
            logical_first_coordinate_y,
            logical_last_coordinate_x,
            logical_last_coordinate_y,
        ];
        let max_text_charcount = max_logical_coordinates
            .iter()
            .map(|max_coordinate| max_coordinate.to_string().len())
            .max()
            .unwrap();

        font.horizontal_advance_max * (max_text_charcount + 4) as i32
    };

    let mut result_bitmap = scaled_bitmap.extended(
        label_padding,
        label_padding,
        label_padding,
        label_padding,
        PixelRGBA::white(),
    );

    // Determine all x label positions
    let label_coords_x = {
        let mut result = Vec::new();
        for bitmap_coord_x in 0..(grid_width + 1) {
            let logical_coord_x = logical_first_coordinate_x + bitmap_coord_x;
            if logical_coord_x % 10 == 0 {
                result.push((bitmap_coord_x, logical_coord_x));
            }
        }

        // Add label for first and last horizontal grid pixel so that we don't mix up a remaining
        // 7, 8 or 9 pixel block with a 10 block
        let pixel_count_in_first_block_horizontal = i32::abs(
            ceil_to_multiple_of_target_i32(logical_first_coordinate_x, 10)
                - logical_first_coordinate_x,
        );
        if pixel_count_in_first_block_horizontal > 3 {
            result.push((0, logical_first_coordinate_x));
        }
        let pixel_count_in_last_block_horizontal = i32::abs(
            floor_to_multiple_of_target_i32(logical_last_coordinate_x, 10)
                - logical_last_coordinate_x,
        );
        if pixel_count_in_last_block_horizontal > 3 {
            result.push((grid_width, logical_last_coordinate_x));
        }

        result
    };

    // Draw x labels
    for (bitmap_coord_x, logical_coord_x) in label_coords_x {
        let text = logical_coord_x.to_string();
        let draw_x = label_padding + grid_cell_size * bitmap_coord_x;
        let draw_pos_top = Vec2i::new(draw_x, label_padding / 2);
        let draw_pos_bottom = Vec2i::new(draw_x, result_bitmap.height - label_padding / 2);

        result_bitmap.draw_text_aligned_in_point(
            font,
            &text,
            1,
            draw_pos_top,
            Vec2i::zero(),
            Some(TextAlignment {
                horizontal: AlignmentHorizontal::Center,
                vertical: AlignmentVertical::Center,
                origin_is_baseline: false,
                ignore_whitespace: false,
            }),
        );
        result_bitmap.draw_text_aligned_in_point(
            font,
            &text,
            1,
            draw_pos_bottom,
            Vec2i::zero(),
            Some(TextAlignment {
                horizontal: AlignmentHorizontal::Center,
                vertical: AlignmentVertical::Center,
                origin_is_baseline: false,
                ignore_whitespace: false,
            }),
        );
    }

    // Determine all y label positions
    let label_coords_y = {
        let mut result = Vec::new();
        for bitmap_coord_y in 0..(grid_height + 1) {
            let logical_coord_y = logical_first_coordinate_y + bitmap_coord_y;
            if logical_coord_y % 10 == 0 {
                result.push((bitmap_coord_y, logical_coord_y));
            }
        }

        // Add label for first and last vertical grid pixel so that we don't mix up a remaining
        // 7, 8 or 9 pixel block with a 10 block
        let pixel_count_in_first_block_vertical = i32::abs(
            ceil_to_multiple_of_target_i32(logical_first_coordinate_y, 10)
                - logical_first_coordinate_y,
        );
        if pixel_count_in_first_block_vertical > 3 {
            result.push((0, logical_first_coordinate_y));
        }
        let pixel_count_in_last_block_vertical = i32::abs(
            floor_to_multiple_of_target_i32(logical_last_coordinate_y, 10)
                - logical_last_coordinate_y,
        );
        if pixel_count_in_last_block_vertical > 3 {
            result.push((grid_height, logical_last_coordinate_y));
        }

        result
    };

    // Draw y labels
    for (bitmap_coord_y, logical_coord_y) in label_coords_y {
        // NOTE: In pixel space our y-coordinates are y-down. We want cartesian y-up so we negate y
        let text = (-logical_coord_y).to_string();
        let draw_y = label_padding + grid_cell_size * bitmap_coord_y;
        let draw_pos_left = Vec2i::new(label_padding / 2, draw_y);
        let draw_pos_right = Vec2i::new(result_bitmap.width - label_padding / 2, draw_y);

        result_bitmap.draw_text_aligned_in_point(
            font,
            &text,
            1,
            draw_pos_left,
            Vec2i::zero(),
            Some(TextAlignment {
                horizontal: AlignmentHorizontal::Center,
                vertical: AlignmentVertical::Center,
                origin_is_baseline: false,
                ignore_whitespace: false,
            }),
        );
        result_bitmap.draw_text_aligned_in_point(
            font,
            &text,
            1,
            draw_pos_right,
            Vec2i::zero(),
            Some(TextAlignment {
                horizontal: AlignmentHorizontal::Center,
                vertical: AlignmentVertical::Center,
                origin_is_baseline: false,
                ignore_whitespace: false,
            }),
        );
    }

    result_bitmap
}

fn create_cross_stitch_pattern(
    bitmap: &Bitmap,
    font_grid_label: &BitmapFont,
    font_segment_index_indicator: &BitmapFont,
    image_filepath: &str,
    output_filename_suffix: &str,
    output_dir_suffix: &str,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
    segment_index: Option<usize>,
    logical_first_coordinate_x: i32,
    logical_first_coordinate_y: i32,
    pattern_type: PatternType,
    add_thick_ten_grid: bool,
    add_origin_grid_bars: bool,
    symbol_mask_color: PixelRGBA,
) {
    let (colorize, add_symbol, use_alphanum) = match pattern_type {
        PatternType::BlackAndWhite => (false, true, false),
        PatternType::Colorized => (true, true, false),
        PatternType::ColorizedNoSymbols => (true, false, false),
        PatternType::PaintByNumbers => (false, true, true),
    };

    let mut scaled_bitmap = Bitmap::new(
        (TILE_SIZE * bitmap.width) as u32,
        (TILE_SIZE * bitmap.height) as u32,
    );
    let scaled_bitmap_width = scaled_bitmap.width;
    let scaled_bitmap_height = scaled_bitmap.height;

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
                let symbol = if use_alphanum {
                    &color_mappings.get(&color).unwrap().symbol_alphanum
                } else {
                    &color_mappings.get(&color).unwrap().symbol
                };

                blit_symbol(
                    symbol,
                    &mut scaled_bitmap,
                    Vec2i::new(TILE_SIZE * x, TILE_SIZE * y),
                    symbol_mask_color,
                );
            }
        }
    }

    // Add 1x1 grid
    for x in 0..bitmap.width {
        scaled_bitmap.draw_rect_filled(TILE_SIZE * x, 0, 1, scaled_bitmap_height, COLOR_GRID_THIN);
    }
    for y in 0..bitmap.height {
        scaled_bitmap.draw_rect_filled(0, TILE_SIZE * y, scaled_bitmap_width, 1, COLOR_GRID_THIN);
    }
    // Close 1x1 grid line on bottom-right bitmap border
    scaled_bitmap.draw_rect_filled(
        scaled_bitmap_width - 1,
        0,
        1,
        scaled_bitmap_height,
        COLOR_GRID_THIN,
    );
    scaled_bitmap.draw_rect_filled(
        0,
        scaled_bitmap_height - 1,
        scaled_bitmap_width,
        1,
        COLOR_GRID_THIN,
    );

    // Add 10x10 grid
    if add_thick_ten_grid {
        for bitmap_x in 0..bitmap.width {
            let logical_x = logical_first_coordinate_x + bitmap_x;
            if logical_x % 10 == 0 {
                scaled_bitmap.draw_rect_filled(
                    TILE_SIZE * bitmap_x,
                    0,
                    2,
                    scaled_bitmap_height,
                    COLOR_GRID_THICK,
                );
            }
        }
        for bitmap_y in 0..bitmap.height {
            let logical_y = logical_first_coordinate_y + bitmap_y;
            if logical_y % 10 == 0 {
                scaled_bitmap.draw_rect_filled(
                    0,
                    TILE_SIZE * bitmap_y,
                    scaled_bitmap_width,
                    2,
                    COLOR_GRID_THICK,
                );
            }
        }
        // Close 10x10 grid line on bottom-right bitmap border if necessary
        if (logical_first_coordinate_x + bitmap.width) % 10 == 0 {
            scaled_bitmap.draw_rect_filled(
                scaled_bitmap_width - 2,
                0,
                2,
                scaled_bitmap_height,
                COLOR_GRID_THICK,
            );
        }
        if (logical_first_coordinate_y + bitmap.height) % 10 == 0 {
            scaled_bitmap.draw_rect_filled(
                0,
                scaled_bitmap_height - 2,
                scaled_bitmap_width,
                2,
                COLOR_GRID_THICK,
            );
        }
    }

    // Add origin grid
    if add_origin_grid_bars {
        let origin_bitmap_coord_x = -logical_first_coordinate_x;
        if 0 < origin_bitmap_coord_x && origin_bitmap_coord_x < bitmap.width {
            draw_origin_line_vertical(&mut scaled_bitmap, TILE_SIZE * origin_bitmap_coord_x);
        }

        let origin_bitmap_coord_y = -logical_first_coordinate_y;
        if 0 < origin_bitmap_coord_y && origin_bitmap_coord_y < bitmap.height {
            draw_origin_line_horizontal(&mut scaled_bitmap, TILE_SIZE * origin_bitmap_coord_y);
        }

        // NOTE: If our origin grid is located on the edge of our image we want to extend our image
        //       so that the origin grid is drawn more clearly visible
        let needs_grid_left = logical_first_coordinate_x == 0;
        let needs_grid_top = logical_first_coordinate_y == 0;
        let needs_grid_right = logical_first_coordinate_x + bitmap.width == 0;
        let needs_grid_bottom = logical_first_coordinate_y + bitmap.height == 0;

        let padding_left = if needs_grid_left { 2 } else { 0 };
        let padding_top = if needs_grid_top { 2 } else { 0 };
        let padding_right = if needs_grid_right { 2 } else { 0 };
        let padding_bottom = if needs_grid_bottom { 2 } else { 0 };

        scaled_bitmap.extend(
            padding_left,
            padding_top,
            padding_right,
            padding_bottom,
            PixelRGBA::white(),
        );

        if needs_grid_left {
            draw_origin_line_vertical(&mut scaled_bitmap, 2);
        }
        if needs_grid_right {
            draw_origin_line_vertical(&mut scaled_bitmap, scaled_bitmap_width);
        }
        if needs_grid_top {
            draw_origin_line_horizontal(&mut scaled_bitmap, 2);
        }
        if needs_grid_bottom {
            draw_origin_line_horizontal(&mut scaled_bitmap, scaled_bitmap_height);
        }
    }

    // Add 10-grid labels
    let final_bitmap = if add_thick_ten_grid {
        // NOTE: At this point the scaled bitmap might not be an exact multiple of the original
        //       bitmap because we may have padded it while drawing the origin grid bars. Therefore
        //       the placement of the labels might be incorrectly shifted by two pixels. This is
        //       okay because it is not really visible and the code complexity to fix this is not
        //       worth it.
        place_grid_labels_in_pattern(
            &scaled_bitmap,
            TILE_SIZE,
            font_grid_label,
            logical_first_coordinate_x,
            logical_first_coordinate_y,
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
    font_grid_label: &BitmapFont,
    font_segment_index_indicator: &BitmapFont,
    image_filepath: &str,
    output_filename_suffix: &str,
    output_dir_suffix: &str,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
    segment_index: Option<usize>,
    logical_first_coordinate_x: i32,
    logical_first_coordinate_y: i32,
    create_paint_by_number_set: bool,
    add_origin_grid_bars: bool,
) {
    rayon::scope(|scope| {
        scope.spawn(|_| {
            create_cross_stitch_pattern(
                &image,
                font_grid_label,
                font_segment_index_indicator,
                &image_filepath,
                &("cross_stitch_colorized_".to_owned() + output_filename_suffix),
                output_dir_suffix,
                &color_mappings,
                segment_index,
                logical_first_coordinate_x,
                logical_first_coordinate_y,
                PatternType::Colorized,
                true,
                add_origin_grid_bars,
                PixelRGBA::white(),
            );
        });
        scope.spawn(|_| {
            create_cross_stitch_pattern(
                &image,
                font_grid_label,
                font_segment_index_indicator,
                &image_filepath,
                &("cross_stitch_".to_owned() + output_filename_suffix),
                output_dir_suffix,
                &color_mappings,
                segment_index,
                logical_first_coordinate_x,
                logical_first_coordinate_y,
                PatternType::BlackAndWhite,
                true,
                add_origin_grid_bars,
                PixelRGBA::white(),
            );
        });
        scope.spawn(|_| {
            create_cross_stitch_pattern(
                &image,
                font_grid_label,
                font_segment_index_indicator,
                &image_filepath,
                &("cross_stitch_colorized_no_symbols_".to_owned() + output_filename_suffix),
                output_dir_suffix,
                &color_mappings,
                segment_index,
                logical_first_coordinate_x,
                logical_first_coordinate_y,
                PatternType::ColorizedNoSymbols,
                true,
                add_origin_grid_bars,
                PixelRGBA::white(),
            );
        });
        if create_paint_by_number_set {
            scope.spawn(|_| {
                create_cross_stitch_pattern(
                    &image,
                    font_grid_label,
                    font_segment_index_indicator,
                    &image_filepath,
                    &("paint_by_numbers_".to_owned() + output_filename_suffix),
                    output_dir_suffix,
                    &color_mappings,
                    segment_index,
                    logical_first_coordinate_x,
                    logical_first_coordinate_y,
                    PatternType::PaintByNumbers,
                    false,
                    false,
                    PixelRGBA::transparent(),
                );
            });
        }
    });
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Image analysis

fn create_color_mappings_from_image(
    image: &Bitmap,
    image_filepath: &str,
    symbols: &[Bitmap],
    symbols_alphanum: &[Bitmap],
    stitch_images_premultiplied_alpha: &[Bitmap],
    stitch_images_luminance_premultiplied_alpha: &[Bitmap],
) -> IndexMap<PixelRGBA, ColorInfo> {
    let mut color_mappings = image_extract_colors_and_counts(&image);

    // Stitch symbols
    assert!(
        symbols.len() >= color_mappings.len(),
        "Not enough symbols to map {} colors found in given image '{}' for cross stitch",
        color_mappings.len(),
        &image_filepath,
    );
    for (entry, symbol) in color_mappings.values_mut().zip(symbols.iter()) {
        entry.symbol = symbol.clone();
    }

    // Alphanum symbols
    assert!(
        symbols_alphanum.len() >= color_mappings.len(),
        "Not enough symbols to map {} colors found in given image '{}' for paint by numbers",
        color_mappings.len(),
        &image_filepath,
    );
    for (entry, symbol_alphanum) in color_mappings.values_mut().zip(symbols_alphanum.iter()) {
        entry.symbol_alphanum = symbol_alphanum.clone();
    }

    // Colorized stitch tiles
    for entry in color_mappings.values_mut() {
        let color = entry.color;
        if color.a != 0 {
            for (stitch_image_premultipllied, stitch_image_luminance_premultiplied) in
                stitch_images_premultiplied_alpha
                    .iter()
                    .zip(stitch_images_luminance_premultiplied_alpha.iter())
            {
                let mut stitch = stitch_image_premultipllied.clone();

                let screen_layer = Bitmap::new_filled(
                    stitch_image_premultipllied.width as u32,
                    stitch_image_premultipllied.height as u32,
                    PixelRGBA::new(105, 109, 128, 255),
                )
                .to_premultiplied_alpha();
                screen_layer.blit_to_alpha_blended_premultiplied(
                    &mut stitch,
                    Vec2i::zero(),
                    false,
                    ColorBlendMode::Screen,
                );

                let color_layer = Bitmap::new_filled(
                    stitch_image_premultipllied.width as u32,
                    stitch_image_premultipllied.height as u32,
                    color,
                )
                .to_premultiplied_alpha();
                color_layer.blit_to_alpha_blended_premultiplied(
                    &mut stitch,
                    Vec2i::zero(),
                    false,
                    ColorBlendMode::Multiply,
                );

                let mut luminosity_layer = stitch_image_luminance_premultiplied.clone();
                let percent = (color.r as f32 + color.g as f32 + color.b as f32) / (3.0 * 255.0);
                for pixel in luminosity_layer.data.iter_mut() {
                    pixel.r /= 6 + (8.0 * percent * percent) as u8;
                    pixel.g /= 6 + (8.0 * percent * percent) as u8;
                    pixel.b /= 6 + (8.0 * percent * percent) as u8;
                    pixel.a /= 6 + (8.0 * percent * percent) as u8;
                }
                luminosity_layer.blit_to_alpha_blended_premultiplied(
                    &mut stitch,
                    Vec2i::zero(),
                    false,
                    ColorBlendMode::Luminosity,
                );

                entry
                    .stitches_premultiplied
                    .push(stitch.masked_by_premultiplied_alpha(&stitch_image_premultipllied));
            }
        }
    }

    color_mappings
}

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
            symbol: Bitmap::new_empty(),
            symbol_alphanum: Bitmap::new_empty(),
            stitches_premultiplied: Vec::new(),
        });
        entry.count += 1;
    }

    // This makes color ramps on the legend more pretty
    color_mappings.sort_by(|color_a, _info_a, color_b, _info_b| {
        PixelRGBA::compare_by_hue_luminosity_saturation(color_a, color_b)
    });

    color_mappings
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Pattern dir creation

fn create_patterns_dir(
    image: &Bitmap,
    image_filepath: &str,
    resources: &Resources,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
) {
    let output_dir_suffix = "";

    let (segment_images, segment_coordinates) =
        image.to_segments(SPLIT_SEGMENT_WIDTH, SPLIT_SEGMENT_HEIGHT);

    rayon::scope(|scope| {
        // Legend
        scope.spawn(|_| {
            create_cross_stitch_legend(
                image.dim(),
                &color_mappings,
                &image_filepath,
                output_dir_suffix,
                &resources.font,
                &segment_coordinates,
            );
        });

        // Create patterns for complete set
        scope.spawn(|_| {
            create_cross_stitch_pattern_set(
                &image,
                &resources.font,
                &resources.font_big,
                &image_filepath,
                "complete",
                output_dir_suffix,
                &color_mappings,
                None,
                0,
                0,
                true,
                false,
            );
        });

        // Create patterns for individual segments if needed
        if segment_images.len() > 1 {
            segment_images
                .par_iter()
                .zip(segment_coordinates.par_iter())
                .enumerate()
                .for_each(|(segment_index, (segment_image, segment_coordinate))| {
                    let label_start_x = SPLIT_SEGMENT_WIDTH * segment_coordinate.x;
                    let label_start_y = SPLIT_SEGMENT_HEIGHT * segment_coordinate.y;

                    create_cross_stitch_pattern_set(
                        segment_image,
                        &resources.font,
                        &resources.font_big,
                        &image_filepath,
                        &format!("segment_{}", segment_index + 1),
                        output_dir_suffix,
                        &color_mappings,
                        Some(segment_index + 1),
                        label_start_x,
                        label_start_y,
                        false,
                        false,
                    );
                });
        }
    });
}

fn create_patterns_dir_centered(
    image: &Bitmap,
    image_filepath: &str,
    resources: &Resources,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
) {
    let output_dir_suffix = "centered";
    let image_center_x = make_even_upwards(image.width) / 2;
    let image_center_y = make_even_upwards(image.height) / 2;

    let (segment_images, segment_coordinates) =
        image.to_segments(SPLIT_SEGMENT_WIDTH, SPLIT_SEGMENT_HEIGHT);

    rayon::scope(|scope| {
        // Legend
        scope.spawn(|_| {
            create_cross_stitch_legend(
                image.dim(),
                &color_mappings,
                &image_filepath,
                output_dir_suffix,
                &resources.font,
                &segment_coordinates,
            );
        });

        // Create patterns for complete set
        scope.spawn(|_| {
            create_cross_stitch_pattern_set(
                &image,
                &resources.font,
                &resources.font_big,
                &image_filepath,
                "complete",
                output_dir_suffix,
                &color_mappings,
                None,
                -image_center_x,
                -image_center_y,
                true,
                true,
            );
        });

        // Create patterns for individual segments if needed
        if segment_images.len() > 1 {
            segment_images
                .par_iter()
                .zip(segment_coordinates.par_iter())
                .enumerate()
                .for_each(|(segment_index, (segment_image, segment_coordinate))| {
                    let logical_first_coordinate_x =
                        SPLIT_SEGMENT_WIDTH * segment_coordinate.x - image_center_x;
                    let logical_first_coordinate_y =
                        SPLIT_SEGMENT_HEIGHT * segment_coordinate.y - image_center_y;

                    create_cross_stitch_pattern_set(
                        segment_image,
                        &resources.font,
                        &resources.font_big,
                        &image_filepath,
                        &format!("segment_{}", segment_index + 1),
                        output_dir_suffix,
                        &color_mappings,
                        Some(segment_index + 1),
                        logical_first_coordinate_x,
                        logical_first_coordinate_y,
                        false,
                        true,
                    );
                });
        }
    });
}

fn create_cross_stitch_pattern_preview(
    bitmap: &Bitmap,
    image_filepath: &str,
    output_filename_suffix: &str,
    output_dir_suffix: &str,
    resources: &Resources,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
) {
    let bitmap = bitmap.extended(10, 10, 10, 10, PixelRGBA::transparent());
    let tile_width = resources
        .stitch_background_image_8x8_premultiplied_alpha
        .width
        / 8;
    let tile_height = resources
        .stitch_background_image_8x8_premultiplied_alpha
        .height
        / 8;

    // Background only
    let mut background_layer = Bitmap::new(
        (tile_width * bitmap.width) as u32,
        (tile_height * bitmap.height) as u32,
    );
    for y in 0..=bitmap.height / 8 {
        for x in 0..=bitmap.width / 8 {
            let pos = Vec2i::new(
                resources
                    .stitch_background_image_8x8_premultiplied_alpha
                    .width
                    * x,
                resources
                    .stitch_background_image_8x8_premultiplied_alpha
                    .height
                    * y,
            );
            resources
                .stitch_background_image_8x8_premultiplied_alpha
                .blit_to(&mut background_layer, pos, true);
        }
    }
    // Write out png image
    let output_filepath = get_image_output_filepath(&image_filepath, output_dir_suffix)
        + "_"
        + output_filename_suffix
        + "_background.png";
    Bitmap::write_to_png_file(&background_layer, &output_filepath);

    // Stitches only
    let mut colored_stitches_layer = Bitmap::new(
        (tile_width * bitmap.width) as u32,
        (tile_height * bitmap.height) as u32,
    );

    let mut random = Random::new_from_seed(1234);
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let color = bitmap.get(x, y);

            // Add stitch
            if color.a != 0 {
                let tile_pos_center =
                    Vec2i::new(tile_width * x, tile_height * y) + (tile_width / 2);
                let stitches = &color_mappings.get(&color).unwrap().stitches_premultiplied;
                let stitches_count = stitches.len();
                let stitch =
                    &stitches[random.u32_bounded_exclusive(stitches_count as u32) as usize];
                let stitch_center = Vec2i::new(stitch.width / 2, stitch.height / 2);
                stitch.blit_to_alpha_blended_premultiplied(
                    &mut colored_stitches_layer,
                    tile_pos_center - stitch_center,
                    true,
                    ColorBlendMode::Normal,
                );
            }
        }
    }
    // Write out png image
    let output_filepath = get_image_output_filepath(&image_filepath, output_dir_suffix)
        + "_"
        + output_filename_suffix
        + "_stitches.png";
    Bitmap::write_to_png_file(
        &colored_stitches_layer.to_unpremultiplied_alpha(),
        &output_filepath,
    );

    // Combined
    let mut combined = background_layer;
    colored_stitches_layer.blit_to_alpha_blended_premultiplied(
        &mut combined,
        Vec2i::zero(),
        false,
        ColorBlendMode::Normal,
    );
    // Write out png image
    let output_filepath = get_image_output_filepath(&image_filepath, output_dir_suffix)
        + "_"
        + output_filename_suffix
        + ".png";
    Bitmap::write_to_png_file(&combined, &output_filepath);
}

fn create_preview_dir(
    image: &Bitmap,
    image_filepath: &str,
    resources: &Resources,
    color_mappings: &IndexMap<PixelRGBA, ColorInfo>,
) {
    let output_dir_suffix = "preview";

    rayon::scope(|scope| {
        // Create stitched preview
        scope.spawn(|_| {
            create_cross_stitch_pattern_preview(
                &image,
                &image_filepath,
                "complete",
                output_dir_suffix,
                resources,
                &color_mappings,
            );
        });
    });
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
            .get_text_bounding_rect(&format!(" {} ", page_count), 1, false)
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
            Some(TextAlignment {
                horizontal: AlignmentHorizontal::Center,
                vertical: AlignmentVertical::Center,
                origin_is_baseline: false,
                ignore_whitespace: false,
            }),
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
                &font,
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
                .map(|chunk| create_legend_block(&font, chunk))
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
        let page_layout_image = create_pattern_page_layout(&font, segment_layout_indices);

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
        let (message, location) = panic_message_split_to_message_and_location(panic_info);
        let final_message = format!("{}\n\nError occured at: {}", message, location);

        show_messagebox("Pixie Stitch Error", &final_message, true);

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
    let (
        stitch_images_premultiplied_alpha,
        stitch_images_luminance_premultiplied_alpha,
        stitch_background_image_8x8_premultiplied_alpha,
    ) = load_stitch_preview_images_premultiplied_alpha();
    let resources = Resources {
        font,
        font_big,
        stitch_background_image_8x8_premultiplied_alpha,
    };

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
        create_image_output_dir(&image_filepath, "preview");

        let image = open_image(&image_filepath);
        let color_mappings = create_color_mappings_from_image(
            &image,
            &image_filepath,
            &symbols,
            &symbols_alphanum,
            &stitch_images_premultiplied_alpha,
            &stitch_images_luminance_premultiplied_alpha,
        );

        rayon::scope(|scope| {
            scope.spawn(|_| {
                create_patterns_dir(&image, &image_filepath, &resources, &color_mappings);
            });
            scope.spawn(|_| {
                create_patterns_dir_centered(&image, &image_filepath, &resources, &color_mappings);
            });
            scope.spawn(|_| {
                create_preview_dir(&image, &image_filepath, &resources, &color_mappings);
            });
        });
    }

    #[cfg(not(debug_assertions))]
    show_messagebox("Pixie Stitch", "Finished creating patterns. Enjoy!", false);
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
        let mut result = Bitmap::new_empty();
        result.width = 128;
        result.height = 128;
        result.data = colors;

        result
    }

    let image = create_test_color_ramp_bitmap();
    Bitmap::write_to_png_file(&image, "test_symbol_contrast.png");

    let mut symbols = collect_symbols();
    while symbols.len() < 128 * 128 {
        symbols = [&symbols[..], &symbols[..]].concat()
    }

    let color_mappings =
        create_color_mappings_from_image(&image, "", &symbols, &vec![], &vec![], &vec![]);

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
        PatternType::Colorized,
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
        let mut result = Bitmap::new_empty();
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
