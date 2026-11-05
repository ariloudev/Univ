//! Entries joined into groups that become objects: a union-find that refuses a join putting two
//! entries of one source, or two groups joined by a candidate link, into one group.

/// Groups of entries, by entry index.
pub struct Groups {
    parent: Vec<usize>,
    /// Each entry's source, by source number.
    own: Vec<u16>,
    /// For each root of a group of more than one entry: the sources of its entries. Left empty,
    /// without an allocation, for an entry alone, which keeps two million entries cheap.
    merged: Vec<Vec<u16>>,
    /// For each root: the entries at the far end of its candidate links.
    links: Vec<Vec<usize>>,
}

/// Why a join was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The groups hold entries of one source.
    SharedSource,
    /// A candidate link joins the groups.
    Linked,
}

impl Groups {
    /// One group per entry, each of the source numbered in `sources`.
    #[must_use]
    pub fn new(sources: &[u16]) -> Self {
        Self {
            parent: (0..sources.len()).collect(),
            own: sources.to_vec(),
            merged: vec![Vec::new(); sources.len()],
            links: vec![Vec::new(); sources.len()],
        }
    }

    /// The root of `i`'s group.
    pub fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    /// The sources of the group rooted at `root`.
    fn sources_of(&self, root: usize) -> &[u16] {
        if self.merged[root].is_empty() {
            std::slice::from_ref(&self.own[root])
        } else {
            &self.merged[root]
        }
    }

    fn sources_disjoint(&self, ra: usize, rb: usize) -> bool {
        let theirs = self.sources_of(rb);
        self.sources_of(ra).iter().all(|s| !theirs.contains(s))
    }

    /// Joins the groups of `a` and `b`, unless a candidate link joins them or they share a
    /// source. Joining a group with itself does nothing.
    ///
    /// # Errors
    ///
    /// Gives why the join was refused.
    pub fn join(&mut self, a: usize, b: usize) -> Result<(), Refused> {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return Ok(());
        }
        if !self.sources_disjoint(ra, rb) {
            return Err(Refused::SharedSource);
        }
        for k in 0..self.links[ra].len() {
            let far = self.links[ra][k];
            if self.find(far) == rb {
                return Err(Refused::Linked);
            }
        }
        self.absorb(ra, rb);
        Ok(())
    }

    /// Joins the groups of `a` and `b` whatever sources they share and whatever links join them:
    /// the extent rule's pieces of one galaxy, which may be several entries of one source. A link
    /// between the two becomes one within the group, and is dropped when objects are built.
    pub fn absorb(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (keep, gone) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent[gone] = keep;
        let mut sources = self.sources_of(keep).to_vec();
        sources.extend_from_slice(self.sources_of(gone));
        self.merged[keep] = sources;
        self.merged[gone] = Vec::new();
        let links = std::mem::take(&mut self.links[gone]);
        self.links[keep].extend(links);
    }

    /// Whether a candidate link joins the groups of `a` and `b`.
    pub fn linked(&mut self, a: usize, b: usize) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        (0..self.links[ra].len()).any(|k| {
            let far = self.links[ra][k];
            self.find(far) == rb
        })
    }

    /// Records a candidate link between the groups of `a` and `b`, which no join may then cross.
    pub fn link(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        self.links[ra].push(b);
        self.links[rb].push(a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_join_never_crosses_a_shared_source_or_a_link() {
        let mut g = Groups::new(&[0, 1, 0, 2, 3]);
        assert_eq!(g.join(0, 1), Ok(()));
        assert_eq!(g.join(1, 2), Err(Refused::SharedSource));
        g.link(3, 4);
        assert_eq!(g.join(0, 3), Ok(()));
        assert_eq!(g.join(1, 4), Err(Refused::Linked));
        assert_eq!(g.find(3), g.find(0));
        assert_ne!(g.find(4), g.find(0));
    }

    #[test]
    fn an_absorb_crosses_a_shared_source_and_a_link() {
        let mut g = Groups::new(&[0, 0, 1]);
        g.link(0, 1);
        assert_eq!(g.join(0, 1), Err(Refused::SharedSource));
        g.absorb(1, 0);
        assert_eq!(g.find(1), g.find(0));
        // An entry of another source still joins; the group keeps every entry's source.
        assert_eq!(g.join(2, 0), Ok(()));
        let root = g.find(2);
        assert_eq!(g.sources_of(root), [0, 0, 1]);
    }
}
