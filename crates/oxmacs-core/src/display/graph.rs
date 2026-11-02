use crate::theme::Highlight;

pub const LANE_COLUMNS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineGraph {
    pub node: Option<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphNode {
    pub lane: usize,
    pub face: Highlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: Option<usize>,
    pub to: Option<usize>,
    pub face: Highlight,
}

impl LineGraph {
    pub fn lanes(&self) -> usize {
        self.edges
            .iter()
            .flat_map(|edge| [edge.from, edge.to])
            .chain([self.node.map(|node| node.lane)])
            .flatten()
            .max()
            .map_or(0, |lane| lane + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_graph_spans_its_widest_lane() {
        let face = Highlight::Nick(0);
        let graph = LineGraph {
            node: Some(GraphNode { lane: 1, face }),
            edges: vec![
                GraphEdge {
                    from: Some(0),
                    to: Some(0),
                    face,
                },
                GraphEdge {
                    from: None,
                    to: Some(3),
                    face,
                },
            ],
        };

        assert_eq!(graph.lanes(), 4);
        assert_eq!(LineGraph::default().lanes(), 0);
    }
}
