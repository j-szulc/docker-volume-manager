use std::ffi::OsStr;
use std::fmt::{Debug};
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use tar::Archive;
use flate2::read::{GzDecoder};

type Error = Box<dyn std::error::Error>;

fn os_str_to_str<T: AsRef<OsStr> + Debug>(os_str: &T) -> String {
    let os_str = os_str.as_ref();
    os_str.to_str().and_then(|str| Some(str.to_string())).unwrap_or_else(|| {
        format!("{:?}", os_str)
   })
}

fn resolve_path(path_str: &str, create: bool) -> Result<(String, String), Error> {

    if create && !Path::new(path_str).exists(){
        std::fs::write(path_str, b"")?;
    }

    let canonical = std::fs::canonicalize(path_str).map_err(|e| {
        format!("Failed to resolve path {}, {}", path_str, e)
    })?;

    let parent = &canonical.parent().ok_or(
        format!("Failed to get parent path of path: {:?}", os_str_to_str(&canonical))
    )?.to_str().ok_or(
        format!("Failed to convert parent path of path to string: {:?}", os_str_to_str(&canonical))
    )?;
    let filename = &canonical.file_name().ok_or(
        format!("Failed to get filename of target path: {:?}", os_str_to_str(&canonical))
    )?.to_str().ok_or(
        format!("Failed to convert filename of target path to string: {:?}", os_str_to_str(&canonical))
    )?;

    Ok((parent.to_string(), filename.to_string()))
}

fn backup(volume_names: Vec<&str>, target_str: &str) -> Result<(), Error>{

    let volume_args = volume_names.iter().map(|volume_name| format!("--volume={}:/input/{}:ro", volume_name, volume_name));

    // create empty file

    let (target_parent_abs, target_filename) = resolve_path(target_str, true)?;

    std::process::Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg(format!("--volume={}:/output", target_parent_abs))
        .args(volume_args)
        .arg("alpine")
        .arg("tar")
        .arg("-czf")
        .arg(format!("/output/{}", target_filename))
        .arg("-C")
        .arg("/input")
        .arg(".")
        .spawn()?
        .wait()?;
    Ok(())
}

fn get_tar_directory_tree(archive_path: &str) -> Result<Vec<PathBuf>, Error>{
    let decoder = GzDecoder::new(std::fs::File::open(archive_path)?);
    let mut archive = Archive::new(decoder);
    let entries = archive.entries()?;
    let entries = entries.collect::<Result<Vec<_>, _>>()?;
    let entries = entries.iter().map(|entry| entry.path().unwrap().into_owned()).collect::<Vec<_>>();
    Ok(entries)
}

trait NegativeIndex<T> {
    fn neg_index(&self, index: isize) -> Option<&T>;
}

impl<T> NegativeIndex<T> for Vec<T> {
    fn neg_index(&self, mut index: isize) -> Option<&T> {
        if index < 0 {
           index = self.len() as isize + index;
           if index < 0 {
               None
           } else {
                self.get(index as usize)
           }
        } else {
            self.get(index as usize)
        }
    }
}

fn get_tar_top_level_list(archive_path: &str) -> Result<Vec<String>, Error>{
    let dir_tree = get_tar_directory_tree(archive_path)?;
    let mut volume_names = dir_tree.iter().map(|path|
        path
            .ancestors()
            .collect::<Vec<_>>()
            .neg_index(-3)
            .and_then(|path| path.file_name())
            .and_then(|path| path.to_str())
    ).filter_map(|path| path)
        .collect::<Vec<_>>();
    // unique
    volume_names.dedup();
    Ok(volume_names.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}


fn restore(source_str: &str) -> Result<(), Error>{

    let volume_names = get_tar_top_level_list(source_str)?;
    let volume_args = volume_names.iter().map(|volume_name| format!("--volume={}:/output/{}:rw", volume_name, volume_name));

    let (source_parent_abs, source_filename) = resolve_path(source_str, false)?;

    std::process::Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg(format!("--volume={}:/input", source_parent_abs))
        .args(volume_args)
        .arg("alpine")
        .arg("tar")
        .arg("-xzf")
        .arg(format!("/input/{}", source_filename))
        .arg("-C")
        .arg("/output")
        .spawn()?
        .wait()?;
    Ok(())
}

#[derive(StructOpt, Debug)]
#[structopt(name = "docker-volume-backup", about = "backup docker volumes")]
enum Opt {
    #[structopt(name = "backup")]
    Backup {
        #[structopt(name = "volume_name", required = true)]
        volume_names: Vec<String>,
        #[structopt(name = "target", required = true)]
        target: String,
    },
    #[structopt(name = "restore")]
    Restore {
        #[structopt(name = "source", required = true)]
        source: String,
    },
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    match opt {
        Opt::Backup { volume_names, target } => backup(volume_names.iter().map(|s| s.as_str()).collect(), &target)?,
        Opt::Restore { source } => restore( &source)?,
    }
    Ok(())
}
