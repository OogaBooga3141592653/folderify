use std::{
    fmt::Display,
    fs::{self, create_dir_all, metadata},
    path::{Path, PathBuf},
};

use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use mktemp::Temp;

const RETINA_SCALE: u32 = 2;

pub const NUM_ICON_CONVERSION_STEPS: u64 = 13;

use crate::{
    command::{
        run_command, run_convert, DEREZ_COMMAND, ICONUTIL_COMMAND, REZ_COMMAND, SETFILE_COMMAND,
        SIPS_COMMAND,
    },
    convert::{density, BlurDown, CommandArgs, CompositingOperation},
    error::{FolderifyError, GeneralError},
    generic_folder_icon::get_folder_icon,
    options::{self, ColorScheme, Options},
    primitives::{Dimensions, Extent, Offset, RGBColor},
};

pub struct ScaledMaskInputs {
    pub icon_size: u32,
    pub mask_dimensions: Dimensions,
    pub offset_y: i32,
}

pub struct BezelInputs {
    pub color: RGBColor,
    pub blur: BlurDown,
    pub mask_operation: CompositingOperation,
    pub opacity: f32,
}

pub struct EngravingInputs {
    pub fill_color: RGBColor,
    pub top_bezel: BezelInputs,
    pub bottom_bezel: BezelInputs,
}

#[derive(Debug)]
pub struct WorkingDir {
    working_dir: Temp,
}

impl WorkingDir {
    pub fn new() -> Self {
        Self {
            working_dir: Temp::new_dir().expect("Couldn't create a temp dir."),
        }
    }

    pub fn icon_conversion(
        &self,
        resolution_prefix: &str,
        multi_progress_bar: Option<MultiProgress>,
    ) -> IconConversion {
        let progress_bar = match multi_progress_bar {
            Some(multi_progress_bar) => {
                let progress_bar =
                    multi_progress_bar.insert(0, ProgressBar::new(NUM_ICON_CONVERSION_STEPS));
                let progress_bar = progress_bar.with_finish(ProgressFinish::AndLeave);
                // TODO share the progress bar style?
                let progress_bar_style = ProgressStyle::with_template(
                    "{bar:12.cyan/blue} | {pos:>2}/{len:2} | {wide_msg}",
                )
                .expect("Could not construct progress bar.")
                .progress_chars("=> ");
                progress_bar.set_style(progress_bar_style);
                progress_bar.set_message("Preparing");
                Some(progress_bar)
            }
            None => None,
        };
        IconConversion {
            working_dir: self.working_dir.as_path().to_owned(),
            resolution_prefix: resolution_prefix.into(),
            progress_bar,
        }
    }

    pub fn open_in_finder(&self) -> Result<(), FolderifyError> {
        let mut open_args = CommandArgs::new();
        open_args.push_path(&self.working_dir);
        run_command("open", &open_args, None)?;
        Ok(())
    }

    pub fn release(self) {
        self.working_dir.release(); // TODO
    }

    pub fn icon_file_with_extension(&self, extension: &str) -> PathBuf {
        self.working_dir
            .as_path()
            .join("icon")
            .with_extension(extension)
    }

    pub fn create_iconset_dir(&self, options: &Options) -> Result<PathBuf, FolderifyError> {
        let iconset_dir = self.icon_file_with_extension("iconset");
        if options.verbose {
            println!("[Iconset] {}", iconset_dir.display());
        };
        if let Err(e) = create_dir_all(&iconset_dir) {
            println!("Error: {}", (e));
            return Err(FolderifyError::General(GeneralError {
                message: "Could not create iconset dir".into(),
            }));
        };
        Ok(iconset_dir)
    }
}

pub enum IconResolution {
    NonRetina16,
    Retina16,
    NonRetina32,
    Retina32,
    NonRetina128,
    Retina128,
    NonRetina256,
    Retina256,
    NonRetina512,
    Retina512,
}

impl IconResolution {
    // TODO: return iterator?
    pub fn values() -> Vec<IconResolution> {
        vec![
            Self::Retina512,
            Self::NonRetina512,
            Self::Retina256,
            Self::NonRetina256,
            Self::Retina128,
            Self::NonRetina128,
            Self::Retina32,
            Self::NonRetina32,
            Self::Retina16,
            Self::NonRetina16,
        ]
    }

    pub fn size(&self) -> u32 {
        match self {
            IconResolution::NonRetina16 => 16,
            IconResolution::Retina16 => 16 * RETINA_SCALE,
            IconResolution::NonRetina32 => 32,
            IconResolution::Retina32 => 32 * RETINA_SCALE,
            IconResolution::NonRetina128 => 128,
            IconResolution::Retina128 => 128 * RETINA_SCALE,
            IconResolution::NonRetina256 => 256,
            IconResolution::Retina256 => 256 * RETINA_SCALE,
            IconResolution::NonRetina512 => 512,
            IconResolution::Retina512 => 512 * RETINA_SCALE,
        }
    }

    pub fn offset_y(&self) -> i32 {
        match self {
            IconResolution::NonRetina16 => -2,
            IconResolution::Retina16 => -2,
            IconResolution::NonRetina32 => -2,
            IconResolution::Retina32 => -3,
            IconResolution::NonRetina128 => -6,
            IconResolution::Retina128 => -12,
            IconResolution::NonRetina256 => -12,
            IconResolution::Retina256 => -24,
            IconResolution::NonRetina512 => -24,
            IconResolution::Retina512 => -48,
        }
    }

    pub fn bottom_bezel_blur_down(&self) -> BlurDown {
        match self {
            IconResolution::NonRetina16 => BlurDown {
                spread_px: 1,
                page_y: 0,
            },
            _ => BlurDown {
                spread_px: 2,
                page_y: 1,
            },
        }
    }

    pub fn bottom_bezel_alpha(&self) -> f32 {
        match self {
            IconResolution::NonRetina16 => 0.5,
            IconResolution::Retina16 => 0.35,
            IconResolution::NonRetina32 => 0.35,
            IconResolution::Retina32 => 0.6,
            IconResolution::NonRetina128 => 0.6,
            IconResolution::Retina128 => 0.6,
            IconResolution::NonRetina256 => 0.6,
            IconResolution::Retina256 => 0.75,
            IconResolution::NonRetina512 => 0.75,
            IconResolution::Retina512 => 0.75,
        }
    }

    pub fn icon_file(&self) -> String {
        format!("icon_{}.png", self)
    }
}

impl Display for IconResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::NonRetina16 => "16x16",
                Self::Retina16 => "16x16@2x",
                Self::NonRetina32 => "32x32",
                Self::Retina32 => "32x32@2x",
                Self::NonRetina128 => "128x128",
                Self::Retina128 => "128x128@2x",
                Self::NonRetina256 => "256x256",
                Self::Retina256 => "256x256@2x",
                Self::NonRetina512 => "512x512",
                Self::Retina512 => "512x512@2x",
            }
        )
    }
}

pub struct IconConversion {
    working_dir: PathBuf,
    resolution_prefix: String,
    pub progress_bar: Option<ProgressBar>,
}

pub struct IconInputs {
    pub color_scheme: ColorScheme,
    pub resolution: IconResolution,
}

impl IconConversion {
    fn step_unincremented(&self, step_description: &str) {
        if let Some(progress_bar) = &self.progress_bar {
            let wide_msg = format!("{:10} | {}", self.resolution_prefix, step_description);
            progress_bar.set_message(wide_msg);
        }
    }

    fn step(&self, step_desciption: &str) {
        if let Some(progress_bar) = &self.progress_bar {
            self.step_unincremented(step_desciption);
            progress_bar.inc(1);
        }
    }

    fn output_path(&self, file_name: &str) -> PathBuf {
        let mut path = self.working_dir.to_path_buf();
        path.push(format!("{}_{}", self.resolution_prefix, file_name));
        path
    }

    pub fn full_mask(
        &self,
        options: &options::Options,
        centering_dimensions: &Dimensions,
    ) -> Result<PathBuf, FolderifyError> {
        let mut args = CommandArgs::new();
        args.background_transparent();
        args.density(density(&options.mask_path, centering_dimensions)?);
        args.push_path(&options.mask_path);
        if !options.no_trim {
            args.trim()
        }
        args.resize(centering_dimensions);
        args.center();
        args.extent(&Extent::no_offset(centering_dimensions));
        let output_path = self.output_path("0.0_FULL_MASK.png");
        args.push_path(&output_path);
        run_convert(&args, None)?;
        Ok(output_path)
    }

    pub fn sized_mask(
        &self,
        input_path: &Path,
        inputs: &ScaledMaskInputs,
    ) -> Result<PathBuf, FolderifyError> {
        let mut args = CommandArgs::new();
        args.background_transparent();
        args.push_path(input_path);
        args.resize(&inputs.mask_dimensions);
        args.center();
        args.extent(&Extent {
            size: Dimensions::square(inputs.icon_size),
            offset: Offset::from_y(inputs.offset_y),
        });
        let output_path = self.output_path("1.0_SIZED_MASK.png");
        args.push_path(&output_path);
        run_convert(&args, None)?;
        Ok(output_path)
    }

    fn simple_operation(
        &self,
        input_path: &Path,
        output_filename: &str,
        f: impl Fn(&mut CommandArgs),
    ) -> Result<PathBuf, FolderifyError> {
        let mut args = CommandArgs::new();
        args.push_path(input_path);
        f(&mut args);
        let file_name = format!("{}.png", output_filename);
        let output_path = self.output_path(&file_name);
        args.push_path(&output_path);
        run_convert(&args, None)?;
        Ok(output_path)
    }

    pub fn engrave(
        &self,
        sized_mask: &Path,
        template_icon: &[u8],
        output_path: &Path,
        inputs: &EngravingInputs,
    ) -> Result<(), FolderifyError> {
        self.step("Creating colorized fill");
        let fill_colorized = self.simple_operation(
            sized_mask,
            "2.1_FILL_COLORIZED",
            |args: &mut CommandArgs| {
                args.fill_colorize(&inputs.fill_color);
            },
        )?;

        self.step("Setting fill opacity");
        let fill =
            self.simple_operation(&fill_colorized, "2.2_FILL", |args: &mut CommandArgs| {
                args.opacity(0.5);
            })?;

        self.step("Complementing mask for top bezel");
        let top_bezel_complement = self.simple_operation(
            sized_mask,
            "3.1_TOP_BEZEL_COMPLEMENT",
            |args: &mut CommandArgs| {
                args.negate();
            },
        )?;

        self.step("Colorizing top bezel");
        let top_bezel_colorized = self.simple_operation(
            &top_bezel_complement,
            "3.2_TOP_BEZEL_COLORIZED",
            |args: &mut CommandArgs| {
                args.fill_colorize(&inputs.top_bezel.color);
            },
        )?;

        self.step("Blurring top bezel");
        let top_bezel_blurred = self.simple_operation(
            &top_bezel_colorized,
            "3.3_TOP_BEZEL_BLURRED",
            |args: &mut CommandArgs| {
                args.blur_down(&inputs.top_bezel.blur);
            },
        )?;

        self.step("Compositing top bezel");
        let top_bezel_masked = self.simple_operation(
            &top_bezel_blurred,
            "3.4_TOP_BEZEL_MASKED",
            |args: &mut CommandArgs| {
                args.mask_down(sized_mask, &inputs.top_bezel.mask_operation);
            },
        )?;

        self.step("Setting top bezel opacity");
        let top_bezel = self.simple_operation(
            &top_bezel_masked,
            "3.5_TOP_BEZEL",
            |args: &mut CommandArgs| {
                args.opacity(inputs.top_bezel.opacity);
            },
        )?;

        self.step("Colorizing bottom bezel");
        let bottom_bezel_colorized = self.simple_operation(
            sized_mask,
            "4.1_BOTTOM_BEZEL_COLORIZED",
            |args: &mut CommandArgs| {
                args.fill_colorize(&inputs.bottom_bezel.color);
            },
        )?;

        self.step("Blurring bottom bezel");
        let bottom_bezel_blurred = self.simple_operation(
            &bottom_bezel_colorized,
            "4.2_BOTTOM_BEZEL_BLURRED",
            |args: &mut CommandArgs| {
                args.blur_down(&inputs.bottom_bezel.blur);
            },
        )?;

        self.step("Compositing bottom bezel");
        let bottom_bezel_masked = self.simple_operation(
            &bottom_bezel_blurred,
            "4.3_BOTTOM_BEZEL_MASKED",
            |args: &mut CommandArgs| {
                args.mask_down(sized_mask, &inputs.bottom_bezel.mask_operation);
            },
        )?;

        self.step("Setting bottom bezel opacity");
        let bottom_bezel = self.simple_operation(
            &bottom_bezel_masked,
            "4.4_BOTTOM_BEZEL",
            |args: &mut CommandArgs| {
                args.opacity(inputs.bottom_bezel.opacity);
            },
        )?;

        self.step("Engraving bezels");
        let mut args = CommandArgs::new();
        args.push("-");
        args.push_path(&bottom_bezel);
        args.composite(&CompositingOperation::dissolve);
        args.push_path(&fill);
        args.composite(&CompositingOperation::dissolve);
        args.push_path(&top_bezel);
        args.composite(&CompositingOperation::dissolve);
        args.push_path(output_path);
        run_convert(&args, Some(template_icon))?;

        self.step("");

        Ok(())
    }

    // TODO
    pub fn icon(
        &self,
        options: &Options,
        full_mask_path: &Path,
        output_path: &Path,
        inputs: &IconInputs,
    ) -> Result<(), FolderifyError> {
        // if options.verbose {
        //     println!("[Starting] {}", inputs.resolution);
        // }

        let size = inputs.resolution.size();
        let offset_y = inputs.resolution.offset_y();

        self.step_unincremented("Sizing mask");
        let sized_mask_path = self
            .sized_mask(
                full_mask_path,
                &ScaledMaskInputs {
                    icon_size: size,
                    mask_dimensions: Dimensions {
                        width: size * 3 / 4,
                        height: size / 2,
                    },
                    offset_y,
                },
            )
            .unwrap();

        // TODO
        let template_icon = get_folder_icon(inputs.color_scheme, &inputs.resolution);

        let fill_color = match inputs.color_scheme {
            ColorScheme::Light => RGBColor::new(8, 134, 206),
            ColorScheme::Dark => RGBColor::new(6, 111, 194),
        };

        let engraved = self.engrave(
            &sized_mask_path,
            template_icon,
            output_path,
            &EngravingInputs {
                fill_color,
                top_bezel: BezelInputs {
                    color: RGBColor::new(58, 152, 208),
                    blur: BlurDown {
                        spread_px: 0,
                        page_y: 2,
                    },
                    mask_operation: CompositingOperation::Dst_In,
                    opacity: 0.5,
                },
                bottom_bezel: BezelInputs {
                    color: RGBColor::new(174, 225, 253),
                    blur: inputs.resolution.bottom_bezel_blur_down(),
                    mask_operation: CompositingOperation::Dst_Out,
                    opacity: inputs.resolution.bottom_bezel_alpha(),
                },
            },
        );
        if options.verbose {
            println!("[{}] {}", options.mask_path.display(), inputs.resolution);
        }
        engraved
    }

    pub fn to_icns(
        &self,
        options: &Options,
        iconset_dir: &Path,
        icns_path: &Path,
    ) -> Result<(), FolderifyError> {
        if options.verbose {
            println!(
                "[{}] Creating the .icns file...",
                options.mask_path.display()
            );
        }
        let mut args = CommandArgs::new();
        args.push_path(iconset_dir);
        args.push("--convert");
        args.push("icns");
        args.push("--output");
        args.push_path(icns_path);
        run_command(ICONUTIL_COMMAND, &args, None)?;
        Ok(())
    }

    pub fn assign_icns(
        &self,
        options: &Options,
        icns_path: &Path,
        target_path: &Path,
    ) -> Result<(), FolderifyError> {
        if options.verbose {
            println!(
                "[{}] Assigning icon to target: {}",
                options.mask_path.display(),
                target_path.display(),
            );
        }
        let target_is_dir = metadata(target_path)
            .expect("Target does not exist!")
            .is_dir(); // TODO: Return `FolderifyError`

        let target_resource_path = if target_is_dir {
            target_path.join("Icon\r")
        } else {
            target_path.to_owned()
        };

        // sips: add an icns resource fork to the icns file
        let mut args = CommandArgs::new();
        args.push("-i");
        args.push_path(icns_path);
        run_command(SIPS_COMMAND, &args, None)?;

        // DeRez: export the icns resource from the icns file
        let mut args = CommandArgs::new();
        args.push("-only");
        args.push("icns");
        args.push_path(icns_path);
        let derezzed = run_command(DEREZ_COMMAND, &args, None)?;
        let derezzed_path = self.output_path("derezzed.data");
        if fs::write(&derezzed_path, derezzed).is_err() {
            return Err(FolderifyError::General(GeneralError {
                message: "Could not write derezzed data".into(),
            }));
        }

        // Rez: add exported icns resource to the resource fork of target/Icon^M
        let mut args = CommandArgs::new();
        args.push("-append");
        args.push_path(&derezzed_path);
        args.push("-o");
        args.push_path(&target_resource_path);
        run_command(REZ_COMMAND, &args, None)?;

        // SetFile: set custom icon attribute
        let mut args = CommandArgs::new();
        args.push("-a");
        args.push("-C");
        args.push_path(target_path);
        run_command(SETFILE_COMMAND, &args, None)?;

        if target_is_dir {
            // SetFile: set invisible file attribute
            let mut args = CommandArgs::new();
            args.push("-a");
            args.push("-V");
            args.push_path(&target_resource_path);
            run_command(SETFILE_COMMAND, &args, None)?;
        }

        Ok(())
    }
}
