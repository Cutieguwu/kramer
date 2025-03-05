use ron::de::{from_reader, SpannedError};
use serde::Deserialize;
use std::fs::File;

use crate::FB_SECTOR_SIZE;


/// Domain, in sectors.
/// Requires sector_size to be provided elsewhere for conversion to bytes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Domain {
    pub start: usize,
    pub end: usize,
}

impl Default for Domain {
    fn default() -> Self {
        Domain { start: 0, end: 1 }
    }
}

impl Domain {
    /// Return length of domain in sectors.
    pub fn len(self) -> usize {
        self.end - self.start
    }
}


/// A map for data stored in memory for processing and saving to disk.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Cluster {
    domain: Domain,
    stage: Stage,
}

impl Default for Cluster {
    fn default() -> Self {
        Cluster {
            domain: Domain::default(),
            stage: Stage::default()
        }
    }
}

impl Cluster {
    /// Breaks apart into a vec of clusters,
    /// each of cluster_size, excepting last.
    pub fn subdivide(&mut self, cluster_len: usize) -> Vec<Cluster> {
        let domain_len = self.domain.len();
        let mut start = self.domain.start;
        let mut clusters: Vec<Cluster> = vec![];

        for _ in 0..(domain_len as f64 / cluster_len as f64).floor() as usize {
            clusters.push(Cluster {
                domain: Domain {
                    start,
                    end: start + cluster_len,
                },
                stage: self.stage,
            });

            start += cluster_len;
        }

        clusters.push(Cluster {
            domain: Domain {
                start,
                end: self.domain.end,
            },
            stage: self.stage,
        });

        clusters
    }

    pub fn set_stage(&mut self, stage: Stage) -> &mut Self {
        self.stage = stage;
        self
    }
}


#[derive(Clone, Copy, Debug, Deserialize, PartialEq, PartialOrd)]
pub enum Stage {
    Untested,
    ForIsolation(u8),
    Damaged,
}

impl Default for Stage {
    fn default() -> Self {
        Stage::Untested
    }
}


#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MapFile {
    pub sector_size: u16,
    pub domain: Domain,
    pub map: Vec<Cluster>,
}

impl TryFrom<File> for MapFile {
    type Error = SpannedError;

    fn try_from(file: File) -> Result<Self, Self::Error> {
        from_reader(file)
    }
}

impl Default for MapFile {
    fn default() -> Self {
        MapFile {
            sector_size: FB_SECTOR_SIZE,
            domain: Domain::default(),
            map: vec![Cluster {
                domain: Domain::default(),
                stage: Stage::Untested,
            }],
        }
    }
}

impl MapFile {
    pub fn new(sector_size: u16) -> Self {
        MapFile::default()
            .set_sector_size(sector_size)
            .to_owned()
    }

    pub fn set_sector_size(&mut self, sector_size: u16) -> &mut Self {
        self.sector_size = sector_size;
        self
    }

    /// Recalculate cluster mappings.
    fn update(&mut self, new_cluster: Cluster) -> &mut Self {
        let mut new_map: Vec<Cluster> = vec![Cluster::from(new_cluster.to_owned())];

        for map_cluster in self.map.iter() {
            let mut map_cluster = *map_cluster;

            // If new_cluster doesn't start ahead and ends short, map_cluster is forgotten.
            if new_cluster.domain.start < map_cluster.domain.start 
            && new_cluster.domain.end < map_cluster.domain.end {
                /* 
                new_cluster overlaps the start of map_cluster, 
                but ends short of map_cluster end.

                ACTION: Crop map_cluster to start at end of new_cluster.
                */

                map_cluster.domain.start = new_cluster.domain.end;
                new_map.push(map_cluster);

            } else if new_cluster.domain.end < map_cluster.domain.end {
                /*
                new_cluster starts within map_cluster domain.

                ACTION: Crop
                */

                let domain_end = map_cluster.domain.end;

                // Crop current object.
                map_cluster.domain.end = new_cluster.domain.start;
                new_map.push(map_cluster);

                if new_cluster.domain.end < map_cluster.domain.end {
                    /*
                    new_cluster is within map_cluster.

                    ACTION: Crop & Fracture map_cluster
                    NOTE: Crop completed above.
                    */

                    new_map.push(Cluster {
                        domain: Domain {
                            start: new_cluster.domain.end,
                            end: domain_end,
                        },
                        stage: map_cluster.stage.to_owned()
                    });
                }
            } else {
                /*
                No overlap.

                ACTION: Transfer
                */

                new_map.push(map_cluster);
            }
        }

        self.map = new_map;
        self
    }

    /// Get current recovery stage.
    pub fn get_stage(&self) -> Stage {
        let mut recover_stage = Stage::Damaged;

        for cluster in self.map.iter() {
            match cluster.stage {
                Stage::Untested => return Stage::Untested,
                Stage::ForIsolation(_) => {
                    if recover_stage == Stage::Damaged
                    || cluster.stage < recover_stage {
                        // Note that recover_stage after first condition is 
                        // only ever Stage::ForIsolation(_), thus PartialEq,
                        // PartialOrd are useful for comparing the internal value.
                        recover_stage = cluster.stage
                    }
                },
                Stage::Damaged => (),
            }
        }

        recover_stage
    }

    /// Get clusters of common stage.
    pub fn get_clusters(&self, stage: Stage) -> Vec<Cluster> {
        self.map.iter()
            .filter_map(|mc| {
                if mc.stage == stage { Some(mc.to_owned()) } else { None }
            })
            .collect()
    }

    /// Defragments cluster groups.
    /// I.E. check forwards every cluster from current until stage changes,
    /// then group at once.
    fn defrag(&mut self) -> &mut Self {
        let mut new_map: Vec<Cluster> = vec![];

        // Fetch first cluster.
        let mut start_cluster = self.map.iter()
            .find(|c| c.domain.start == 0)
            .unwrap();
        
        // Even though this would be initialized by its first read,
        // the compiler won't stop whining, and idk how to assert that to it.
        let mut end_cluster = Cluster::default();
        let mut new_cluster: Cluster;

        let mut stage_common: bool;
        let mut is_finished = false;

        while !is_finished {
            stage_common = true;

            // Start a new cluster based on the cluster following
            // the end of last new_cluster.
            new_cluster = start_cluster.to_owned();

            // While stage is common, and not finished,
            // find each trailing cluster.
            while stage_common && !is_finished {
                end_cluster = start_cluster.to_owned();

                if end_cluster.domain.end != self.domain.end {
                    start_cluster = self.map.iter()
                        .find(|c| end_cluster.domain.end == c.domain.start)
                        .unwrap();

                    stage_common = new_cluster.stage == start_cluster.stage
                } else {
                    is_finished = true;
                }
            }

            // Set the new ending, encapsulating any clusters of common stage.
            new_cluster.domain.end = end_cluster.domain.end;
            new_map.push(new_cluster);
        }

        self.map = new_map;
        self
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // Test for Cluster::subdivide()

    // Test for MapFile::update()

    // Test for MapFile::get_stage()
    #[test]
    fn test_get_stage() {
        use std::vec;

        let mut mf = MapFile::default();
        let mut mf_stage = mf.get_stage();

        // If this fails here, there's something SERIOUSLY wrong.
        assert!(
            mf_stage == Stage::Untested,
            "Determined stage to be {:?}, when {:?} was expeccted.",
            mf_stage, Stage::Untested
        );


        let stages = vec![
            Stage::Damaged,
            Stage::ForIsolation(1),
            Stage::ForIsolation(0),
            Stage::Untested,
        ];

        mf.map = vec![];

        for stage in stages {
            mf.map.push(*Cluster::default().set_stage(stage));

            mf_stage = mf.get_stage();

            assert!(
                stage == mf_stage,
                "Expected stage to be {:?}, determined {:?} instead.",
                stage, mf_stage
            )
        }
    }

    // Test for MapFile::get_clusters()
    #[test]
    fn test_get_clusters() {
        let mut mf = MapFile::default();

        mf.map = vec![
            *Cluster::default().set_stage(Stage::Damaged),
            *Cluster::default().set_stage(Stage::ForIsolation(0)),
            *Cluster::default().set_stage(Stage::ForIsolation(1)),
            Cluster::default(),
            Cluster::default(),
            *Cluster::default().set_stage(Stage::ForIsolation(1)),
            *Cluster::default().set_stage(Stage::ForIsolation(0)),
            *Cluster::default().set_stage(Stage::Damaged),
        ];

        let stages = vec![
            Stage::Damaged,
            Stage::ForIsolation(1),
            Stage::ForIsolation(0),
            Stage::Untested,
        ];

        for stage in stages {
            let expected = vec![
                *Cluster::default().set_stage(stage),
                *Cluster::default().set_stage(stage),
            ];
            let recieved = mf.get_clusters(stage);

            assert!(
                expected == recieved,
                "Expected clusters {:?}, got {:?}.",
                expected, recieved
            )
        }
    }

    // Test for MapFile::defrag()
    #[test]
    fn test_defrag() {
        let mut mf = MapFile {
            sector_size: 1,
            domain: Domain { start: 0, end: 8 },
            map: vec![
                Cluster {
                    domain: Domain { start: 0, end: 1 },
                    stage: Stage::Untested,
                },
                Cluster {
                    domain: Domain { start: 1, end: 2 },
                    stage: Stage::Untested,
                },
                Cluster {
                    domain: Domain { start: 2, end: 3 },
                    stage: Stage::Untested,
                },
                Cluster {
                    domain: Domain { start: 3, end: 4 },
                    stage: Stage::ForIsolation(0),
                },
                Cluster {
                    domain: Domain { start: 4, end: 5 },
                    stage: Stage::ForIsolation(0),
                },
                Cluster {
                    domain: Domain { start: 5, end: 6 },
                    stage: Stage::ForIsolation(1),
                },
                Cluster {
                    domain: Domain { start: 6, end: 7 },
                    stage: Stage::ForIsolation(0),
                },
                Cluster {
                    domain: Domain { start: 7, end: 8 },
                    stage: Stage::Damaged,
                },
            ],
        };

        let expected = vec![
            Cluster {
                domain: Domain { start: 0, end: 3 },
                stage: Stage::Untested,
            },
            Cluster {
                domain: Domain { start: 3, end: 5 },
                stage: Stage::ForIsolation(0),
            },
            Cluster {
                domain: Domain { start: 5, end: 6 },
                stage: Stage::ForIsolation(1),
            },
            Cluster {
                domain: Domain { start: 6, end: 7 },
                stage: Stage::ForIsolation(0),
            },
            Cluster {
                domain: Domain { start: 7, end: 8 },
                stage: Stage::Damaged,
            },
        ];

        mf.defrag();

        let recieved = mf.map;

        assert!(
            expected == recieved,
            "Expected {:?} after defragging, got {:?}.",
            expected, recieved
        )
    }
}