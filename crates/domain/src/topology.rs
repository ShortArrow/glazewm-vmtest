//! The set of displays the guest OS sees.

/// A rectangle in virtual-screen coordinates, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// The key that orders rectangles left to right, then top to bottom.
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Whether `other` lies entirely within this rectangle; touching an edge counts as within.
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
}

impl std::fmt::Display for Rect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}x{} at ({}, {})",
            self.width, self.height, self.x, self.y
        )
    }
}

/// One display as the guest OS reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub id: String,
    pub bounds: Rect,
    pub working_area: Rect,
    pub dpi: u32,
    pub is_primary: bool,
}

/// The guest's displays, in no particular order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Topology {
    displays: Vec<Display>,
}

impl Topology {
    /// Creates a topology from displays in any order.
    pub fn new(displays: Vec<Display>) -> Self {
        Self { displays }
    }

    /// Returns the displays ordered by position: left to right, then top to bottom.
    ///
    /// The index into the result is the position glazewm's `bind_to_monitor` and
    /// zebar's `monitorSelection` of type `index` refer to.
    pub fn by_position(&self) -> Vec<&Display> {
        let mut displays: Vec<&Display> = self.displays.iter().collect();
        displays.sort_by_key(|d| d.bounds.position());
        displays
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: &str, x: i32, y: i32) -> Display {
        let bounds = Rect {
            x,
            y,
            width: 1920,
            height: 1080,
        };
        Display {
            id: id.into(),
            bounds,
            working_area: bounds,
            dpi: 96,
            is_primary: false,
        }
    }

    fn ids(displays: Vec<&Display>) -> Vec<&str> {
        displays.into_iter().map(|d| d.id.as_str()).collect()
    }

    #[test]
    fn orders_displays_left_to_right() {
        let topology = Topology::new(vec![display("right", 1920, 0), display("left", 0, 0)]);

        assert_eq!(ids(topology.by_position()), ["left", "right"]);
    }

    #[test]
    fn orders_displays_sharing_a_left_edge_top_to_bottom() {
        let topology = Topology::new(vec![display("lower", 0, 1080), display("upper", 0, 0)]);

        assert_eq!(ids(topology.by_position()), ["upper", "lower"]);
    }

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 1280,
        height: 760,
    };

    #[test]
    fn contains_a_rectangle_inside_it_up_to_its_edges() {
        assert!(AREA.contains(&AREA));
        assert!(AREA.contains(&Rect {
            x: 20,
            y: 20,
            width: 600,
            height: 400
        }));
    }

    #[test]
    fn does_not_contain_a_rectangle_reaching_past_any_edge() {
        let past_right = Rect {
            x: 1000,
            y: 0,
            width: 281,
            height: 100,
        };
        let past_top = Rect {
            x: 0,
            y: -1,
            width: 100,
            height: 100,
        };

        assert!(!AREA.contains(&past_right));
        assert!(!AREA.contains(&past_top));
    }

    #[test]
    fn shows_as_size_at_position() {
        assert_eq!(AREA.to_string(), "1280x760 at (0, 0)");
    }

    #[test]
    fn has_no_positions_without_displays() {
        assert!(Topology::default().by_position().is_empty());
    }
}
