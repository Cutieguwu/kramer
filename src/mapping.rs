use ron::de::{from_reader, SpannedError};
use serde::Deserialize;
use std::fs::File;

use crate::FB_SECTOR_SIZE;


/// Domain, in sectors.
/// Requires sector_size to be provided elsewhere for conversion to bytes.
#[derive(Clone, Copy, Debug, Deserialize)]
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
#[derive(Clone, Debug, Deserialize)]
pub struct Cluster {
    data: Option<Vec<u8>>,
    domain: Domain,
    stage: Stage,
}

impl Default for Cluster {
    fn default() -> Self {
        Cluster {
            data: None,
            domain: Domain::default(),
            stage: Stage::default()
        }
    }
}


/// Map for data stored on disk.
/// Rather have a second cluster type than inflating size
/// of output map by defining Option::None constantly.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct MapCluster {
    pub domain: Domain,
    pub stage: Stage,
}

impl Default for MapCluster {
    fn default() -> Self {
        MapCluster { domain: Domain::default(), stage: Stage::default() }
    }
}

impl From<Cluster> for MapCluster {
    fn from(cluster: Cluster) -> Self {
        MapCluster {
            domain: cluster.domain,
            stage: cluster.stage,
        }
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


#[derive(Clone, Debug, Deserialize)]
pub struct MapFile {
    pub sector_size: u16,
    pub domain: Domain,
    pub map: Vec<MapCluster>,
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
            map: vec![MapCluster {
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
    fn update(&mut self, new_cluster: Cluster) {
        let mut new_map: Vec<MapCluster> = vec![MapCluster::from(new_cluster.to_owned())];

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

                    new_map.push(MapCluster {
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
    pub fn get_clusters(self, stage: Stage) -> Vec<MapCluster> {
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
        let mut new_map: Vec<MapCluster> = vec![];

        let mut pos: usize = 0;

        // Fetch first cluster.
        let mut start_cluster = *self.map.iter()
            .find(|c| c.domain.start == pos)
            .unwrap();
        // Even though this would be initialized by its first read,
        // the compiler won't stop whining, and idk how to assert that to it.
        let mut end_cluster = MapCluster::default();
        let mut new_cluster: MapCluster;

        let mut stage_common: bool;

        while pos < self.domain.end {
            stage_common = true;

            // Start a new cluster based on the cluster following
            // the end of last new_cluster.
            new_cluster = start_cluster;

            // While stage is common, find each trailing cluster.
            while stage_common {
                // start_cluster was of common stage to end_cluster.
                end_cluster = start_cluster;

                start_cluster = *self.map.iter()
                    .find(|c| end_cluster.domain.end == c.domain.start)
                    .unwrap();

                stage_common = new_cluster.stage == start_cluster.stage
            }

            // Set the new ending, encapsulating any clusters of common stage.
            new_cluster.domain.end = end_cluster.domain.end;
            pos = new_cluster.domain.end;
            new_map.push(new_cluster);
        }

        self.map = new_map;
        self
    }
}