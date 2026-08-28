use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clap::{Parser, ValueEnum};
use rust_lib_sw_cutter::engine::alpha::AlphaMode;
use rust_lib_sw_cutter::engine::cutter::{self, CutEvent, CutParams};
use rust_lib_sw_cutter::engine::planner::{Resample, Scheme};

#[derive(Parser)]
#[command(name = "swcutter", version, about = "GeoTIFF 瓦片切片工具")]
struct Cli {
    /// 源 GeoTIFF 文件路径
    #[arg(short, long)]
    source: PathBuf,

    /// 输出目录
    #[arg(short, long)]
    output: PathBuf,

    /// 瓦片尺寸（默认 256）
    #[arg(short, long, default_value_t = 256)]
    tile_size: u32,

    /// 最小缩放级别
    #[arg(long)]
    zmin: Option<u32>,

    /// 最大缩放级别
    #[arg(long)]
    zmax: Option<u32>,

    /// 瓦片编号方案
    #[arg(long, value_enum, default_value_t = SchemeArg::Xyz)]
    scheme: SchemeArg,

    /// 透明模式
    #[arg(long, value_enum, default_value_t = AlphaArg::Keep)]
    alpha: AlphaArg,

    /// 重采样方法
    #[arg(long, value_enum, default_value_t = ResampleArg::Bilinear)]
    resample: ResampleArg,

    /// 跳过完全透明的瓦片
    #[arg(long, default_value_t = false)]
    skip_empty: bool,

    /// 使用 Mercator 绝对级别模式（需要 GeoTIFF 地理参考）
    #[arg(long, default_value_t = false)]
    mercator: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum SchemeArg {
    Xyz,
    Tms,
}

impl From<SchemeArg> for Scheme {
    fn from(s: SchemeArg) -> Self {
        match s {
            SchemeArg::Xyz => Scheme::Xyz,
            SchemeArg::Tms => Scheme::Tms,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum AlphaArg {
    Keep,
    Threshold,
    Colorkey,
}

impl From<AlphaArg> for AlphaMode {
    fn from(a: AlphaArg) -> Self {
        match a {
            AlphaArg::Keep => AlphaMode::Keep,
            AlphaArg::Threshold => AlphaMode::Threshold { below: 128 },
            AlphaArg::Colorkey => AlphaMode::ColorKey {
                r: 0,
                g: 0,
                b: 0,
                tolerance: 30,
            },
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ResampleArg {
    Nearest,
    Bilinear,
}

impl From<ResampleArg> for Resample {
    fn from(r: ResampleArg) -> Self {
        match r {
            ResampleArg::Nearest => Resample::Nearest,
            ResampleArg::Bilinear => Resample::Bilinear,
        }
    }
}

fn main() {
    let cli = Cli::parse();

    if !cli.source.is_file() {
        eprintln!("错误: 源文件不存在: {}", cli.source.display());
        std::process::exit(1);
    }

    let params = CutParams {
        source: cli.source,
        output: cli.output,
        tile_size: cli.tile_size,
        zmin: cli.zmin,
        zmax: cli.zmax,
        scheme: cli.scheme.into(),
        alpha: cli.alpha.into(),
        resample: cli.resample.into(),
        skip_empty: cli.skip_empty,
        mercator: cli.mercator,
        preview_overlays: None,
    };

    eprintln!("开始切片...");
    eprintln!("  源文件: {}", params.source.display());
    eprintln!("  输出: {}", params.output.display());
    eprintln!("  瓦片尺寸: {}", params.tile_size);
    eprintln!("  级别: {:?} - {:?}", params.zmin, params.zmax);
    eprintln!("  方案: {:?}", params.scheme);
    eprintln!("  Mercator: {}", params.mercator);

    let sink = Arc::new(Mutex::new(move |ev: CutEvent| {
        match ev {
            CutEvent::Start { total_tiles } => {
                eprintln!("总瓦片数: {total_tiles}");
            }
            CutEvent::LevelStart { level, .. } => {
                eprintln!("  处理级别 L{level}...");
            }
            CutEvent::Progress(p) => {
                if p.tiles_done % 100 == 0 || p.tiles_done == p.total_tiles {
                    eprintln!(
                        "  L{}: {}/{} 瓦片, {} bytes, {}ms",
                        p.level, p.tiles_done, p.total_tiles, p.bytes_written, p.elapsed_ms
                    );
                }
            }
            CutEvent::Done(summary) => {
                eprintln!("切片完成!");
                eprintln!("  总瓦片: {}", summary.total_tiles);
                eprintln!("  总字节: {}", summary.bytes_written);
                eprintln!("  耗时: {}ms", summary.elapsed_ms);
                if !summary.errors.is_empty() {
                    eprintln!("  错误: {}", summary.errors.len());
                    for err in &summary.errors {
                        eprintln!("    - {err}");
                    }
                }
            }
        }
    }));

    let summary = cutter::run_cut(&params, sink);

    if summary.errors.is_empty() {
        eprintln!("成功! 共 {} 瓦片, {} bytes", summary.total_tiles, summary.bytes_written);
        std::process::exit(0);
    } else {
        eprintln!("失败! {} 个错误", summary.errors.len());
        std::process::exit(1);
    }
}
