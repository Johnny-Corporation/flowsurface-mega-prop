use super::{
    CLUSTER_CELL_GAP, CLUSTER_FOOTER_ROWS, CscalpDom, MONO_CHAR_ADVANCE, ROW_HEIGHT, TEXT_SIZE,
    types::{ColumnRanges, VisibleRow, cluster_column_geometry, cluster_totals},
};
use data::panel::cscalp_dom::{ClusterCell, ClusterColumn};
use exchange::unit::Price;
use iced::{
    Alignment, Point, Rectangle, Size,
    widget::canvas::{Frame, Path, Stroke},
};

impl CscalpDom {
    pub(super) fn draw_row_guides(
        &self,
        frame: &mut Frame,
        y: f32,
        width: f32,
        color: iced::Color,
    ) {
        frame.fill_rectangle(Point::new(0.0, y), Size::new(width, 1.0), color);
    }

    pub(super) fn draw_cluster_cells(
        &self,
        frame: &mut Frame,
        y: f32,
        price: Price,
        clusters: &[ClusterColumn],
        max_cluster_qty: f32,
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
        cols: &ColumnRanges,
    ) {
        if clusters.is_empty() || max_cluster_qty <= 0.0 {
            return;
        }

        let Some((first_x, col_width)) = cluster_column_geometry(cols.clusters, clusters.len())
        else {
            return;
        };

        for (idx, cluster) in clusters.iter().enumerate() {
            let Some(cell) = cluster.cells.get(&price).copied() else {
                continue;
            };
            let total = f32::from(cell.total());
            if total <= 0.0 {
                continue;
            }

            let x = first_x + idx as f32 * col_width;
            self.draw_cluster_cell(
                frame,
                (x, x + col_width - CLUSTER_CELL_GAP),
                y,
                cell,
                max_cluster_qty,
                bid_color,
                ask_color,
                text_color,
            );
        }
    }

    pub(super) fn draw_cluster_grid_row(
        &self,
        frame: &mut Frame,
        y: f32,
        clusters: &[ClusterColumn],
        divider_color: iced::Color,
        cols: &ColumnRanges,
    ) {
        if clusters.is_empty() {
            return;
        }

        let Some((first_x, col_width)) = cluster_column_geometry(cols.clusters, clusters.len())
        else {
            return;
        };

        for idx in 0..clusters.len() {
            let x = first_x + idx as f32 * col_width;
            let x_end = x + col_width - CLUSTER_CELL_GAP;
            if x_end <= x {
                continue;
            }

            frame.fill_rectangle(
                Point::new(x, y + 1.0),
                Size::new((x_end - x).max(0.0), (ROW_HEIGHT - 2.0).max(0.0)),
                divider_color.scale_alpha(0.045),
            );

            let outline = Path::rectangle(
                Point::new(x.floor() + 0.5, y.floor() + 0.5),
                Size::new((x_end - x).max(0.0), (ROW_HEIGHT - 1.0).max(0.0)),
            );
            frame.stroke(
                &outline,
                Stroke::default()
                    .with_color(divider_color.scale_alpha(0.20))
                    .with_width(1.0),
            );

            frame.fill_rectangle(
                Point::new(x.floor() + 0.5, y),
                Size::new(1.0, ROW_HEIGHT),
                divider_color.scale_alpha(0.42),
            );
            frame.fill_rectangle(
                Point::new(x, y.floor() + 0.5),
                Size::new((x_end - x).max(0.0), 1.0),
                divider_color.scale_alpha(0.20),
            );
        }
    }

    fn draw_cluster_cell(
        &self,
        frame: &mut Frame,
        (x_start, x_end): (f32, f32),
        y: f32,
        cell: ClusterCell,
        max_cluster_qty: f32,
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
    ) {
        let total = f32::from(cell.total());
        if total <= 0.0 || max_cluster_qty <= 0.0 {
            return;
        }

        let sell = f32::from(cell.sell_qty);
        let buy = f32::from(cell.buy_qty);
        let dominant = if sell > buy { ask_color } else { bid_color };
        let intensity = (total / max_cluster_qty).clamp(0.0, 1.0);
        let cell_width = (x_end - x_start).max(0.0);
        let inner_x = x_start + 1.0;
        let inner_y = y + 1.0;
        let inner_w = (cell_width - 2.0).max(0.0);
        let inner_h = (ROW_HEIGHT - 2.0).max(0.0);
        if inner_w <= 0.0 || inner_h <= 0.0 {
            return;
        }
        let fill_w = (inner_w * intensity).max(2.0).min(inner_w);

        frame.fill_rectangle(
            Point::new(inner_x, inner_y),
            Size::new(inner_w, inner_h),
            dominant.scale_alpha(0.06),
        );

        frame.fill_rectangle(
            Point::new(inner_x, inner_y),
            Size::new(fill_w, inner_h),
            dominant.scale_alpha(0.28 + intensity * 0.36),
        );

        let outline = Path::rectangle(Point::new(inner_x, inner_y), Size::new(inner_w, inner_h));
        frame.stroke(
            &outline,
            Stroke::default()
                .with_color(dominant.scale_alpha(0.35 + intensity * 0.35))
                .with_width(1.0),
        );

        if buy > 0.0 && sell > 0.0 {
            let other_side = if sell > buy { bid_color } else { ask_color };
            frame.fill_rectangle(
                Point::new(inner_x, inner_y + inner_h - 2.0),
                Size::new(fill_w, 2.0),
                other_side.scale_alpha(0.58),
            );
        }

        let qty_txt = self.format_quantity(cell.total());
        let label_width = qty_txt.chars().count() as f32 * TEXT_SIZE * MONO_CHAR_ADVANCE + 8.0;
        if x_end - x_start >= label_width {
            Self::draw_cell_text(
                frame,
                &qty_txt,
                x_end - 4.0,
                y,
                text_color.scale_alpha(0.88),
                Alignment::End,
            );
        }
    }

    pub(super) fn draw_cluster_footer(
        &self,
        frame: &mut Frame,
        bounds: Rectangle,
        clusters: &[ClusterColumn],
        bid_color: iced::Color,
        ask_color: iced::Color,
        text_color: iced::Color,
        muted_text_color: iced::Color,
        footer_bg: iced::Color,
        divider_color: iced::Color,
        cols: &ColumnRanges,
    ) {
        if clusters.is_empty() {
            return;
        }
        let footer_h = ROW_HEIGHT * CLUSTER_FOOTER_ROWS;
        if bounds.height <= footer_h {
            return;
        }

        let Some((first_x, col_width)) = cluster_column_geometry(cols.clusters, clusters.len())
        else {
            return;
        };

        let footer_y = bounds.height - footer_h;
        frame.fill_rectangle(
            Point::new(cols.clusters.0, footer_y),
            Size::new((cols.clusters.1 - cols.clusters.0).max(0.0), footer_h),
            footer_bg.scale_alpha(0.94),
        );
        frame.fill_rectangle(
            Point::new(cols.clusters.0, footer_y),
            Size::new((cols.clusters.1 - cols.clusters.0).max(0.0), 1.0),
            divider_color,
        );

        for (idx, cluster) in clusters.iter().enumerate() {
            let x = first_x + idx as f32 * col_width;
            let x_end = x + col_width - CLUSTER_CELL_GAP;
            let (buy, sell) = cluster_totals(cluster);
            let total = buy + sell;
            let delta = buy - sell;

            Self::draw_cell_text(
                frame,
                &self.format_quantity(total),
                x_end - 4.0,
                footer_y,
                text_color,
                Alignment::End,
            );

            let delta_color = if delta.units >= 0 {
                bid_color
            } else {
                ask_color
            };
            Self::draw_cell_text(
                frame,
                &self.format_quantity(delta),
                x_end - 4.0,
                footer_y + ROW_HEIGHT,
                delta_color,
                Alignment::End,
            );

            let label = cluster
                .bucket
                .format_utc("%M:%S")
                .unwrap_or_else(|| "--:--".to_string());
            Self::draw_cell_text(
                frame,
                &label,
                x_end - 4.0,
                footer_y + ROW_HEIGHT * 2.0,
                muted_text_color,
                Alignment::End,
            );
        }
    }

    pub(super) fn visible_cluster_max_qty(
        &self,
        clusters: &[ClusterColumn],
        visible_rows: &[VisibleRow],
    ) -> f32 {
        let mut max_qty = 0.0_f32;
        for row in visible_rows {
            let Some(price) = row.row.price() else {
                continue;
            };
            for cluster in clusters {
                if let Some(cell) = cluster.cells.get(&price) {
                    max_qty = max_qty.max(f32::from(cell.total()));
                }
            }
        }
        max_qty
    }
}
