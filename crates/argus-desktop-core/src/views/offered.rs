//! Which saved views the Feature stage offers for the feature it has open (CHECKOUTS-CODE-VIEWS-
//! SAVED-18): the ones scoped to it or attached to it. The page's `forFeature`, kept here so the
//! rule is tested once.

use crate::views::saved::PageSaved;

/// The views scoped to or attached to feature `number` of `repository`; none without a repository.
pub fn for_feature(views: &[PageSaved], repository: Option<u64>, number: i64) -> Vec<&PageSaved> {
    let Some(repository) = repository else { return Vec::new() };
    views.iter().filter(|view| view.features.iter().any(|f| f.repository == repository && f.number == number)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::docks::Panel;
    use crate::views::saved::{PageFeature, PageScope};
    use argus_protocol::AgentView;

    fn view(id: &str, features: &[(u64, i64)]) -> PageSaved {
        PageSaved {
            id: id.into(),
            panel: Panel::Saved(id.into()),
            scope: PageScope { rank: 2, label: "argus #7".into() },
            here: true,
            features: features.iter().map(|&(repository, number)| PageFeature { repository, number }).collect(),
            recipe: None,
            sources: Vec::new(),
            built_at: 0,
            built_commit: None,
            stale: false,
            view: AgentView { id: id.into(), title: id.into(), ..Default::default() },
            diagrams: Vec::new(),
            refreshing: false,
            pressed: Vec::new(),
            under: None,
        }
    }

    fn ids(found: Vec<&PageSaved>) -> Vec<&str> {
        found.into_iter().map(|view| view.id.as_str()).collect()
    }

    #[test]
    fn a_feature_is_offered_the_views_scoped_or_attached_to_it() {
        // SAVED-18.
        let views = [view("a", &[(1, 7)]), view("b", &[(1, 8)]), view("c", &[(1, 8), (1, 7)]), view("d", &[])];
        assert_eq!(ids(for_feature(&views, Some(1), 7)), ["a", "c"]);
    }

    #[test]
    fn a_feature_number_of_another_repository_is_not_the_feature() {
        let views = [view("a", &[(2, 7)])];
        assert!(for_feature(&views, Some(1), 7).is_empty());
    }

    #[test]
    fn no_repository_offers_nothing() {
        let views = [view("a", &[(1, 7)])];
        assert!(for_feature(&views, None, 7).is_empty());
    }
}
