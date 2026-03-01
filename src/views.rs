use crate::{
    applet::{Message, SystemMonitorApplet, base_background},
    color::Color,
    components::{
        bar::PercentageBar,
        gpu::GpuData,
        run::{SimpleHistoryChart, SuperimposedHistoryChart},
    },
    config::{
        ComponentConfig, CpuView, IconColorMode, IoView, PaddingOption, PercentView,
        QuickIntervalProfile, TextFormatMode,
    },
};
use cosmic::{
    Apply, Element, Renderer, Theme,
    iced::{Alignment, Padding, Pixels, Size, padding},
    widget::{Column, Container, Row, button, container, icon, list_column, settings, text, toggler},
};
use std::sync::LazyLock;
use sysinfo::Cpu;

// Iconos SVG embebidos (siempre visibles, no dependen del tema)
static ICON_CPU: LazyLock<cosmic::widget::icon::Handle> =
    LazyLock::new(|| icon::from_svg_bytes(include_bytes!("../res/icons/cpu.svg")));
static ICON_MEMORY: LazyLock<cosmic::widget::icon::Handle> =
    LazyLock::new(|| icon::from_svg_bytes(include_bytes!("../res/icons/memory.svg")));
static ICON_DISK: LazyLock<cosmic::widget::icon::Handle> =
    LazyLock::new(|| icon::from_svg_bytes(include_bytes!("../res/icons/disk.svg")));
static ICON_GPU: LazyLock<cosmic::widget::icon::Handle> =
    LazyLock::new(|| icon::from_svg_bytes(include_bytes!("../res/icons/gpu.svg")));
static ICON_NETWORK: LazyLock<cosmic::widget::icon::Handle> =
    LazyLock::new(|| icon::from_svg_bytes(include_bytes!("../res/icons/network.svg")));

fn sized_container<'a>(
    content: impl Into<Element<'a, Message>>,
    size: Size,
) -> Container<'a, Message, Theme> {
    container(content.into())
        .width(size.width)
        .height(size.height)
        .style(base_background)
}

fn icon_style(theme: &Theme, mode: IconColorMode) -> container::Style {
    let color = match mode {
        IconColorMode::Auto => cosmic::iced::Color::from(theme.cosmic().primary.on),
        IconColorMode::White => cosmic::iced::Color::WHITE,
        IconColorMode::Black => cosmic::iced::Color::BLACK,
    };
    container::Style {
        icon_color: Some(color),
        ..container::Style::default()
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    #[allow(clippy::cast_precision_loss)]
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{}{}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1}{}", size, UNITS[unit_index])
    }
}

fn format_percentage(current: u64, total: u64) -> String {
    if total == 0 {
        "0.0%".to_string()
    } else {
        #[allow(clippy::cast_precision_loss)]
        let percentage = (current as f64 / total as f64) * 100.0;
        format!("{percentage:.1}%")
    }
}

pub fn format_cpu_tooltip(usage: f32, temperature: Option<f32>) -> String {
    match temperature {
        Some(temp) => format!("CPU: {usage:.1}%\nTemp: {temp:.1}°C"),
        None => format!("CPU: {usage:.1}%"),
    }
}

fn format_gpu_tooltip(gpu_index: usize, gpu_data: &GpuData) -> String {
    format!(
        "{}\n{}",
        format_gpu_usage_tooltip(gpu_index, gpu_data),
        format_gpu_vram_tooltip(gpu_index, gpu_data)
    )
}

fn format_gpu_usage_tooltip(gpu_index: usize, gpu_data: &GpuData) -> String {
    format!("GPU{} Usage: {}%", gpu_index, gpu_data.usage)
}

fn format_gpu_vram_tooltip(gpu_index: usize, gpu_data: &GpuData) -> String {
    let vram_percentage = format_percentage(gpu_data.used_vram, gpu_data.total_vram);
    format!(
        "GPU{} VRAM: {}/{} ({})",
        gpu_index,
        format_bytes(gpu_data.used_vram),
        format_bytes(gpu_data.total_vram),
        vram_percentage
    )
}

impl SystemMonitorApplet {
    fn format_mem_tooltip(&self) -> String {
        format!(
            "{}\n{}",
            self.format_ram_tooltip(),
            self.format_swap_tooltip()
        )
    }

    fn format_ram_tooltip(&self) -> String {
        let used = self.sys.used_memory();
        let total = self.sys.total_memory();
        let percentage = format_percentage(used, total);
        format!(
            "RAM: {} / {} ({})",
            format_bytes(used),
            format_bytes(total),
            percentage
        )
    }

    fn format_swap_tooltip(&self) -> String {
        let used = self.sys.used_swap();
        let total = self.sys.total_swap();
        if total == 0 {
            "Swap: Not available".to_string()
        } else {
            let percentage = format_percentage(used, total);
            format!(
                "Swap: {} / {} ({})",
                format_bytes(used),
                format_bytes(total),
                percentage
            )
        }
    }

    fn format_network_tooltip(&self) -> String {
        format!(
            "{}\n{}",
            self.format_network_tooltip_inner(false),
            self.format_network_tooltip_inner(true)
        )
    }

    fn format_network_tooltip_inner(&self, is_upload: bool) -> String {
        let history = if is_upload {
            &self.upload
        } else {
            &self.download
        };
        let current_rate = history.iter().last().copied().unwrap_or(0);
        let direction = if is_upload { "Upload" } else { "Download" };
        format!("{}: {}/s", direction, format_bytes(current_rate))
    }

    fn format_disk_tooltip(&self) -> String {
        format!(
            "{}\n{}",
            self.format_disk_tooltip_inner(false),
            self.format_disk_tooltip_inner(true)
        )
    }

    fn format_disk_tooltip_inner(&self, is_write: bool) -> String {
        let history = if is_write {
            &self.disk_write
        } else {
            &self.disk_read
        };
        let current_rate = history.iter().last().copied().unwrap_or(0);
        let operation = if is_write { "Write" } else { "Read" };
        format!("Disk {}: {}/s", operation, format_bytes(current_rate))
    }

    fn disk_space_used(&self) -> (u64, u64) {
        self.disks.iter().fold((0u64, 0u64), |(total, used), d| {
            let t = d.total_space();
            let avail = d.available_space();
            (total + t, used + t.saturating_sub(avail))
        })
    }

    fn format_disk_space_tooltip(&self) -> String {
        let (total, used) = self.disk_space_used();
        let pct = if total > 0 {
            format_percentage(used, total)
        } else {
            "N/A".to_string()
        };
        format!(
            "Disco: {} / {} ({})",
            format_bytes(used),
            format_bytes(total),
            pct
        )
    }

    fn uses_labeled_text(&self) -> bool {
        matches!(self.config.ui.text_format_mode, TextFormatMode::Labeled)
    }

    fn visible_cpu_temperature(&self) -> Option<f32> {
        if self.config.ui.show_cpu_temperature {
            self.cpu_temperature
        } else {
            None
        }
    }

    pub fn main_content(&'_ self) -> Element<'_, Message> {
        let enabled = self.config.ui.enabled_modules;
        let item_iter = self
            .config
            .components
            .iter()
            .filter_map(|module| match module {
                ComponentConfig::Cpu(vis) if enabled.cpu => Some(self.cpu_view(vis)),
                ComponentConfig::Mem(vis) if enabled.mem => Some(self.mem_view(vis)),
                ComponentConfig::Net(vis) if enabled.net => Some(self.net_view(vis)),
                ComponentConfig::Disk(vis) if enabled.disk => Some(self.disk_view(vis)),
                ComponentConfig::Gpu(vis) if enabled.gpu => Some(self.gpu_view(vis)),
                _ => None,
            })
            .map(|elements| self.panel_collection(elements, self.config.layout.inner_spacing, 0.0));

        let items = self.panel_collection(item_iter, self.config.layout.spacing, self.padding());
        container(items).into()
    }

    pub fn main_button<'a>(&self, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        button::custom(content).padding(0).on_press(Message::TogglePopup).into()
    }

    pub fn settings_popup_view(&'_ self) -> Element<'_, Message> {
        let modules = self.config.ui.enabled_modules;
        let text_format_row = Row::with_children(vec![
            button::custom(text("Compacto").size(14))
                .on_press(Message::SetTextFormatMode(TextFormatMode::Compact))
                .into(),
            button::custom(text("Etiquetas").size(14))
                .on_press(Message::SetTextFormatMode(TextFormatMode::Labeled))
                .into(),
        ])
        .spacing(8.0);

        let icon_mode_row = Row::with_children(vec![
            button::custom(text("Auto").size(14))
                .on_press(Message::SetIconColorMode(IconColorMode::Auto))
                .into(),
            button::custom(text("Blanco").size(14))
                .on_press(Message::SetIconColorMode(IconColorMode::White))
                .into(),
            button::custom(text("Negro").size(14))
                .on_press(Message::SetIconColorMode(IconColorMode::Black))
                .into(),
        ])
        .spacing(8.0);

        let interval_row = Row::with_children(vec![
            button::custom(text("Rápido").size(14))
                .on_press(Message::SetQuickIntervalProfile(QuickIntervalProfile::Fast))
                .into(),
            button::custom(text("Normal").size(14))
                .on_press(Message::SetQuickIntervalProfile(QuickIntervalProfile::Normal))
                .into(),
            button::custom(text("Lento").size(14))
                .on_press(Message::SetQuickIntervalProfile(QuickIntervalProfile::Slow))
                .into(),
        ])
        .spacing(8.0);

        let content = list_column()
            .padding(8)
            .spacing(0)
            .add(settings::item(
                "CPU",
                container(
                    toggler(modules.cpu).on_toggle(|v| Message::ToggleModuleCpu(v)),
                ),
            ))
            .add(settings::item(
                "RAM",
                container(
                    toggler(modules.mem).on_toggle(|v| Message::ToggleModuleMem(v)),
                ),
            ))
            .add(settings::item(
                "Red",
                container(
                    toggler(modules.net).on_toggle(|v| Message::ToggleModuleNet(v)),
                ),
            ))
            .add(settings::item(
                "Disco",
                container(
                    toggler(modules.disk).on_toggle(|v| Message::ToggleModuleDisk(v)),
                ),
            ))
            .add(settings::item(
                "GPU",
                container(
                    toggler(modules.gpu).on_toggle(|v| Message::ToggleModuleGpu(v)),
                ),
            ))
            .add(settings::item(
                "Mostrar temperatura CPU",
                container(
                    toggler(self.config.ui.show_cpu_temperature)
                        .on_toggle(|v| Message::ToggleCpuTemperature(v)),
                ),
            ))
            .add(settings::item("Formato de texto", text_format_row))
            .add(settings::item("Color de iconos", icon_mode_row))
            .add(settings::item("Intervalo", interval_row));

        Element::from(self.core.applet.popup_container(content))
    }

    fn size_aspect_ratio(&self, aspect_ratio: f32) -> Size {
        let (bounds_width, bounds_height) = self.core.applet.suggested_window_size();
        let padding = self.padding();

        #[allow(clippy::cast_precision_loss)]
        if self.is_horizontal() {
            let height = bounds_height.get() as f32 - padding.vertical();
            Size {
                width: height * aspect_ratio,
                height,
            }
        } else {
            let width = bounds_width.get() as f32 - padding.horizontal();
            Size {
                width,
                height: width * aspect_ratio,
            }
        }
    }

    pub fn padding(&self) -> Padding {
        match self.config.layout.padding {
            PaddingOption::Suggested => {
                Into::<[u16; 2]>::into(self.core.applet.suggested_padding(false)).into()
            }
            PaddingOption::Custom(p) => p.into(),
        }
    }

    fn aspect_ratio_container<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        aspect_ratio: f32,
    ) -> Container<'a, Message, Theme, Renderer> {
        sized_container(content, self.size_aspect_ratio(aspect_ratio))
    }

    fn aspect_ratio_container_with_padding<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        aspect_ratio: f32,
    ) -> Container<'a, Message, Theme, Renderer> {
        let size = self.size_aspect_ratio(aspect_ratio);
        sized_container(content, size).padding(padding::top(size.height / 5.0).bottom(0.0))
    }

    fn is_horizontal(&self) -> bool {
        self.core.applet.is_horizontal()
    }

    pub fn panel_collection<'a>(
        &self,
        elements: impl IntoIterator<Item = impl Into<Element<'a, Message>>>,
        spacing: impl Into<Pixels>,
        padding: impl Into<Padding>,
    ) -> Element<'a, Message> {
        if self.is_horizontal() {
            Row::with_children(elements.into_iter().map(Into::into))
                .spacing(spacing)
                .align_y(Alignment::Center)
                .padding(padding)
                .into()
        } else {
            Column::with_children(elements.into_iter().map(Into::into))
                .spacing(spacing)
                .align_x(Alignment::Center)
                .padding(padding)
                .into()
        }
    }

    fn maybe_tooltip<'a>(
        &self,
        container: Container<'a, Message, Theme>,
        tooltip_text: String,
    ) -> Element<'a, Message> {
        if self.config.tooltip_enabled {
            self.core
                .applet
                .applet_tooltip(container, tooltip_text, false, Message::Surface, None)
                .into()
        } else {
            container.into()
        }
    }

    fn single_run_view<'a, T>(
        &self,
        content: SimpleHistoryChart<'a, T>,
        tooltip_text: String,
        aspect_ratio: f32,
    ) -> Element<'a, Message>
    where
        SimpleHistoryChart<'a, T>: Into<Element<'a, Message>>,
    {
        self.aspect_ratio_container(content, aspect_ratio)
            .apply(|c| self.maybe_tooltip(c, tooltip_text))
    }

    fn double_run_view<'a>(
        &'a self,
        content: SuperimposedHistoryChart<'a>,
        tooltip_text: String,
        aspect_ratio: f32,
    ) -> Element<'a, Message> {
        self.aspect_ratio_container_with_padding(content, aspect_ratio)
            .apply(|c| self.maybe_tooltip(c, tooltip_text))
    }

    fn single_bar_view(
        &'_ self,
        data: u64,
        max: u64,
        color: &Color,
        tooltip_text: String,
        aspect_ratio: f32,
    ) -> Element<'_, Message> {
        self.aspect_ratio_container(
            PercentageBar::from_pair(self.is_horizontal(), data, max, *color),
            aspect_ratio,
        )
        .apply(|c| self.maybe_tooltip(c, tooltip_text))
    }

    fn double_bar_view<'a>(
        &'a self,
        content_left: Element<'a, Message>,
        content_right: Element<'a, Message>,
        tooltip_text: String,
        spacing: f32,
    ) -> Element<'a, Message> {
        self.panel_collection(vec![content_left, content_right], spacing, 0.0)
            .apply(container)
            .style(base_background)
            .apply(|c| self.maybe_tooltip(c, tooltip_text))
    }

    pub fn cpu_bar_view(
        &'_ self,
        data: f32,
        color: &Color,
        tooltip_text: String,
        aspect_ratio: f32,
    ) -> Element<'_, Message> {
        self.aspect_ratio_container(
            PercentageBar::new(self.is_horizontal(), data, *color),
            aspect_ratio,
        )
        .apply(|c| self.maybe_tooltip(c, tooltip_text))
    }

    fn text_view<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        tooltip_text: String,
    ) -> Element<'a, Message> {
        let icon_mode = self.config.ui.icon_color_mode;
        container(content)
            .style(move |theme| icon_style(theme, icon_mode))
            .apply(|c| self.maybe_tooltip(c, tooltip_text))
            .into()
    }

    fn icon_text_view<'a>(
        &self,
        icon_handle: &'static LazyLock<cosmic::widget::icon::Handle>,
        text_content: impl ToString,
        tooltip_text: String,
    ) -> Element<'a, Message> {
        let content = Row::with_children(vec![
            icon::icon((*icon_handle).clone())
                .size(14)
                .into(),
            text(text_content.to_string()).size(14).into(),
        ])
        .spacing(4.0)
        .align_y(Alignment::Center);
        self.text_view(content, tooltip_text)
    }

    fn cpu_text_view(&'_ self) -> Element<'_, Message> {
        let usage = self.sys.global_cpu_usage();
        let temperature = self.visible_cpu_temperature();
        let labeled = self.uses_labeled_text();
        let cpu_text = match (labeled, temperature) {
            (true, Some(temp)) => format!("CPU {usage:.1}% | {temp:.1}°C"),
            (true, None) => format!("CPU {usage:.1}%"),
            (false, Some(temp)) => format!("{usage:.1}% | {temp:.1}°C"),
            (false, None) => format!("{usage:.1}%"),
        };
        self.icon_text_view(
            &ICON_CPU,
            cpu_text,
            format_cpu_tooltip(usage, temperature),
        )
    }

    pub fn cpu_view(&'_ self, vis: &[CpuView]) -> Vec<Element<'_, Message>> {
        vis.iter()
            .map(|v| match v {
                CpuView::Text => self.cpu_text_view(),
                CpuView::BarGlobal {
                    aspect_ratio,
                    color,
                } => self.cpu_bar_view(
                    self.sys.global_cpu_usage(),
                    color,
                    format_cpu_tooltip(self.sys.global_cpu_usage(), self.visible_cpu_temperature()),
                    *aspect_ratio,
                ),
                CpuView::BarCores {
                    aspect_ratio,
                    color,
                    spacing,
                    sorting,
                } => {
                    let mut cpus: Vec<_> = self.sys.cpus().iter().map(Cpu::cpu_usage).collect();
                    cpus.sort_by(sorting.method());

                    let bars: Vec<Element<_>> = cpus
                        .into_iter()
                        .enumerate()
                        .map(|(core_idx, usage)| {
                            self.cpu_bar_view(
                                usage,
                                color,
                                format!("CPU{core_idx}: {usage:.1}%"),
                                *aspect_ratio,
                            )
                        })
                        .collect();

                    self.panel_collection(bars, *spacing, 0.0)
                        .apply(container)
                        .style(base_background)
                        .apply(|c| {
                            self.maybe_tooltip(
                                c,
                                format!("CPU: {} cores total", self.sys.cpus().len()),
                            )
                        })
                }
                CpuView::Run {
                    aspect_ratio,
                    color,
                } => self.single_run_view(
                    SimpleHistoryChart::new(&self.global_cpu, 100.0, *color),
                    format_cpu_tooltip(self.sys.global_cpu_usage(), self.visible_cpu_temperature()),
                    *aspect_ratio,
                ),
            })
            .collect::<Vec<Element<_>>>()
    }

    fn single_percent_text_view(
        &'_ self,
        icon_handle: &'static LazyLock<cosmic::widget::icon::Handle>,
        label: &str,
        current: u64,
        total: u64,
        tooltip_text: String,
    ) -> Element<'_, Message> {
        let value = format_percentage(current, total);
        let text_content = if self.uses_labeled_text() {
            format!("{label} {value}")
        } else {
            value
        };
        self.icon_text_view(icon_handle, text_content, tooltip_text)
    }

    fn percent_text_view(&'_ self) -> Element<'_, Message> {
        let ram_pct = format_percentage(self.sys.used_memory(), self.sys.total_memory());
        let swap_total = self.sys.total_swap();
        let swap_pct = if swap_total == 0 {
            "N/A".to_string()
        } else {
            format_percentage(self.sys.used_swap(), swap_total)
        };
        let text_content = if self.uses_labeled_text() {
            format!("RAM {} | SWAP {}", ram_pct, swap_pct)
        } else {
            format!("{} | {}", ram_pct, swap_pct)
        };
        self.icon_text_view(&ICON_MEMORY, text_content, self.format_mem_tooltip())
    }

    pub fn mem_view(&'_ self, vis: &[PercentView]) -> Vec<Element<'_, Message>> {
        vis.iter()
            .map(|v| match v {
                PercentView::Text => self.percent_text_view(),
                PercentView::TextLeft => self.single_percent_text_view(
                    &ICON_MEMORY,
                    "RAM",
                    self.sys.used_memory(),
                    self.sys.total_memory(),
                    self.format_ram_tooltip(),
                ),
                PercentView::TextRight => {
                    let total = self.sys.total_swap();
                    if total == 0 {
                        self.icon_text_view(
                            &ICON_MEMORY,
                            if self.uses_labeled_text() {
                                "SWAP N/A"
                            } else {
                                "N/A"
                            },
                            "Swap: Not available".to_string(),
                        )
                    } else {
                        self.single_percent_text_view(
                            &ICON_MEMORY,
                            "SWAP",
                            self.sys.used_swap(),
                            total,
                            self.format_swap_tooltip(),
                        )
                    }
                }
                PercentView::Bar {
                    color_left,
                    color_right,
                    spacing,
                    aspect_ratio,
                } => self.double_bar_view(
                    self.single_bar_view(
                        self.sys.used_memory(),
                        self.sys.total_memory(),
                        color_left,
                        self.format_ram_tooltip(),
                        *aspect_ratio,
                    ),
                    self.single_bar_view(
                        self.sys.used_swap(),
                        self.sys.total_swap(),
                        color_right,
                        self.format_swap_tooltip(),
                        *aspect_ratio,
                    ),
                    self.format_mem_tooltip(),
                    *spacing,
                ),
                PercentView::BarLeft {
                    color,
                    aspect_ratio,
                } => self.single_bar_view(
                    self.sys.used_memory(),
                    self.sys.total_memory(),
                    color,
                    self.format_ram_tooltip(),
                    *aspect_ratio,
                ),

                PercentView::BarRight {
                    color,
                    aspect_ratio,
                } => self.single_bar_view(
                    self.sys.used_swap(),
                    self.sys.total_swap(),
                    color,
                    self.format_swap_tooltip(),
                    *aspect_ratio,
                ),
                PercentView::Run {
                    aspect_ratio,
                    color_back,
                    color_front,
                } => self.double_run_view(
                    SuperimposedHistoryChart::new(
                        &self.swap,
                        self.sys.total_swap(),
                        color_front,
                        &self.ram,
                        self.sys.total_memory(),
                        color_back,
                    ),
                    self.format_mem_tooltip(),
                    *aspect_ratio,
                ),
                PercentView::RunBack {
                    color,
                    aspect_ratio,
                } => self.single_run_view(
                    SimpleHistoryChart::new(&self.ram, self.sys.total_memory(), *color),
                    self.format_ram_tooltip(),
                    *aspect_ratio,
                ),
                PercentView::RunFront {
                    color,
                    aspect_ratio,
                } => self.single_run_view(
                    SimpleHistoryChart::new(&self.swap, self.sys.total_swap(), *color),
                    self.format_swap_tooltip(),
                    *aspect_ratio,
                ),
            })
            .collect()
    }

    fn io_text_view(
        &'_ self,
        icon_handle: &'static LazyLock<cosmic::widget::icon::Handle>,
        back_rate: u64,
        front_rate: u64,
        back_label: &str,
        front_label: &str,
        tooltip_text: String,
    ) -> Element<'_, Message> {
        let labeled = self.uses_labeled_text();
        let text_content = format!(
            "{} {}/s | {} {}/s",
            back_label,
            format_bytes(back_rate),
            front_label,
            format_bytes(front_rate),
        );
        let compact = format!("{}/s | {}/s", format_bytes(back_rate), format_bytes(front_rate));
        self.icon_text_view(
            icon_handle,
            if labeled { text_content } else { compact },
            tooltip_text,
        )
    }

    pub fn net_view(&'_ self, vis: &[IoView]) -> Vec<Element<'_, Message>> {
        let download = self.download.iter().last().copied().unwrap_or(0);
        let upload = self.upload.iter().last().copied().unwrap_or(0);
        vis.iter()
            .map(|v| match v {
                IoView::TextSpace => {
                    // TextSpace es para Disk; en Net ignorar (no aplica)
                    let labeled = self.uses_labeled_text();
                    self.io_text_view(
                        &ICON_NETWORK,
                        download,
                        upload,
                        if labeled { "DOWN" } else { "↓" },
                        if labeled { "UP" } else { "↑" },
                        self.format_network_tooltip(),
                    )
                }
                IoView::Text => {
                    let labeled = self.uses_labeled_text();
                    self.io_text_view(
                        &ICON_NETWORK,
                        download,
                        upload,
                        if labeled { "DOWN" } else { "↓" },
                        if labeled { "UP" } else { "↑" },
                        self.format_network_tooltip(),
                    )
                }
                IoView::TextBack => self.icon_text_view(
                    &ICON_NETWORK,
                    if self.uses_labeled_text() {
                        format!("DOWN {}/s", format_bytes(download))
                    } else {
                        format!("↓ {}/s", format_bytes(download))
                    },
                    self.format_network_tooltip_inner(false),
                ),
                IoView::TextFront => self.icon_text_view(
                    &ICON_NETWORK,
                    if self.uses_labeled_text() {
                        format!("UP {}/s", format_bytes(upload))
                    } else {
                        format!("↑ {}/s", format_bytes(upload))
                    },
                    self.format_network_tooltip_inner(true),
                ),
                IoView::Run {
                    aspect_ratio,
                    color_front,
                    color_back,
                } => self.double_run_view(
                    SuperimposedHistoryChart::new_linked(
                        &self.upload,
                        color_front,
                        &self.download,
                        color_back,
                    ),
                    self.format_network_tooltip(),
                    *aspect_ratio,
                ),
                IoView::RunBack {
                    color,
                    aspect_ratio,
                } => self.single_run_view(
                    SimpleHistoryChart::auto_max(&self.download, *color),
                    self.format_swap_tooltip(),
                    *aspect_ratio,
                ),
                IoView::RunFront {
                    color,
                    aspect_ratio,
                } => self.single_run_view(
                    SimpleHistoryChart::auto_max(&self.upload, *color),
                    self.format_swap_tooltip(),
                    *aspect_ratio,
                ),
            })
            .collect()
    }

    pub fn disk_view(&'_ self, vis: &[IoView]) -> Vec<Element<'_, Message>> {
        let read = self.disk_read.iter().last().copied().unwrap_or(0);
        let write = self.disk_write.iter().last().copied().unwrap_or(0);
        let (disk_total, disk_used) = self.disk_space_used();
        let disk_pct = if disk_total > 0 {
            format_percentage(disk_used, disk_total)
        } else {
            "0%".to_string()
        };
        vis.iter()
            .map(|v| match v {
                IoView::TextSpace => self.icon_text_view(
                    &ICON_DISK,
                    if self.uses_labeled_text() {
                        format!("DISK {}", disk_pct)
                    } else {
                        disk_pct.clone()
                    },
                    self.format_disk_space_tooltip(),
                ),
                IoView::Text => self.io_text_view(
                    &ICON_DISK,
                    read,
                    write,
                    if self.uses_labeled_text() { "READ" } else { "R" },
                    if self.uses_labeled_text() { "WRITE" } else { "W" },
                    self.format_disk_tooltip(),
                ),
                IoView::TextBack => self.icon_text_view(
                    &ICON_DISK,
                    if self.uses_labeled_text() {
                        format!("READ {}/s", format_bytes(read))
                    } else {
                        format!("R {}/s", format_bytes(read))
                    },
                    self.format_disk_tooltip_inner(false),
                ),
                IoView::TextFront => self.icon_text_view(
                    &ICON_DISK,
                    if self.uses_labeled_text() {
                        format!("WRITE {}/s", format_bytes(write))
                    } else {
                        format!("W {}/s", format_bytes(write))
                    },
                    self.format_disk_tooltip_inner(true),
                ),
                IoView::Run {
                    color_front,
                    color_back,
                    aspect_ratio,
                } => self.double_run_view(
                    SuperimposedHistoryChart::new_linked(
                        &self.disk_write,
                        color_front,
                        &self.disk_read,
                        color_back,
                    ),
                    self.format_disk_tooltip(),
                    *aspect_ratio,
                ),
                IoView::RunBack {
                    color,
                    aspect_ratio,
                } => self.single_run_view(
                    SimpleHistoryChart::auto_max(&self.disk_read, *color),
                    self.format_swap_tooltip(),
                    *aspect_ratio,
                ),
                IoView::RunFront {
                    color,
                    aspect_ratio,
                } => self.single_run_view(
                    SimpleHistoryChart::auto_max(&self.disk_write, *color),
                    self.format_swap_tooltip(),
                    *aspect_ratio,
                ),
            })
            .collect()
    }

    fn gpu_text_view(
        &'_ self,
        gpu_index: usize,
        data: &GpuData,
    ) -> Element<'_, Message> {
        let usage_pct = format!("{}%", data.usage);
        let vram_pct = format_percentage(data.used_vram, data.total_vram);
        let text_content = if self.uses_labeled_text() {
            format!("GPU {} | VRAM {}", usage_pct, vram_pct)
        } else {
            format!("{} | {}", usage_pct, vram_pct)
        };
        self.icon_text_view(
            &ICON_GPU,
            text_content,
            format_gpu_tooltip(gpu_index, data),
        )
    }

    pub fn gpu_view(&'_ self, vis: &[PercentView]) -> Vec<Element<'_, Message>> {
        self.gpus
            .data()
            .iter()
            .enumerate()
            .flat_map(|(idx, data)| {
                vis.iter()
                    .map(|v| match v {
                        PercentView::Text => self.gpu_text_view(idx, data),
                        PercentView::TextLeft => self.icon_text_view(
                            &ICON_GPU,
                            if self.uses_labeled_text() {
                                format!("GPU {}%", data.usage)
                            } else {
                                format!("{}%", data.usage)
                            },
                            format_gpu_usage_tooltip(idx, data),
                        ),
                        PercentView::TextRight => self.single_percent_text_view(
                            &ICON_GPU,
                            "VRAM",
                            data.used_vram,
                            data.total_vram,
                            format_gpu_vram_tooltip(idx, data),
                        ),
                        PercentView::Bar {
                            color_left,
                            color_right,
                            spacing,
                            aspect_ratio,
                        } => self.double_bar_view(
                            self.single_bar_view(
                                data.usage,
                                100,
                                color_left,
                                format_gpu_usage_tooltip(idx, data),
                                *aspect_ratio,
                            ),
                            self.single_bar_view(
                                data.used_vram,
                                data.total_vram,
                                color_right,
                                format_gpu_vram_tooltip(idx, data),
                                *aspect_ratio,
                            ),
                            format_gpu_tooltip(idx, data),
                            *spacing,
                        ),
                        PercentView::BarLeft {
                            color,
                            aspect_ratio,
                        } => self.single_bar_view(
                            data.usage,
                            100,
                            color,
                            format_gpu_usage_tooltip(idx, data),
                            *aspect_ratio,
                        ),
                        PercentView::BarRight {
                            color,
                            aspect_ratio,
                        } => self.single_bar_view(
                            data.used_vram,
                            data.total_vram,
                            color,
                            format_gpu_vram_tooltip(idx, data),
                            *aspect_ratio,
                        ),

                        PercentView::Run {
                            aspect_ratio,
                            color_back,
                            color_front,
                        } => self.double_run_view(
                            SuperimposedHistoryChart::new(
                                &self.vram[idx],
                                data.total_vram,
                                color_front,
                                &self.gpu_usage[idx],
                                100,
                                color_back,
                            ),
                            format_gpu_tooltip(idx, data),
                            *aspect_ratio,
                        ),
                        PercentView::RunBack {
                            color,
                            aspect_ratio,
                        } => self.single_run_view(
                            SimpleHistoryChart::new(&self.gpu_usage[idx], 100, *color),
                            format_gpu_usage_tooltip(idx, data),
                            *aspect_ratio,
                        ),
                        PercentView::RunFront {
                            color,
                            aspect_ratio,
                        } => self.single_run_view(
                            SimpleHistoryChart::new(&self.vram[idx], data.total_vram, *color),
                            format_gpu_usage_tooltip(idx, data),
                            *aspect_ratio,
                        ),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
