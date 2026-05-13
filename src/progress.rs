use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub struct Reporter {
    multi: MultiProgress,
    enabled: bool,
}

impl Reporter {
    pub fn new(enabled: bool) -> Self {
        Self {
            multi: MultiProgress::new(),
            enabled,
        }
    }

    pub fn new_folder_bar(&self, folder: &str, total: u64) -> ProgressBar {
        if !self.enabled {
            return ProgressBar::hidden();
        }
        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(
            ProgressStyle::with_template(
                "{prefix:>20} [{bar:30.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        pb.set_prefix(folder.to_string());
        pb
    }
}
