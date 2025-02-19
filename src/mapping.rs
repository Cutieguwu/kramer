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
#[allow(unused)]
#[derive(Clone, Debug, Deserialize)]
pub struct Cluster {
    data: Option<Vec<u8>>,
    domain: Domain,
    status: Status,
}

impl Default for Cluster {
    fn default() -> Self {
        Cluster {
            data: None,
            domain: Domain::default(),
            status: Status::default()
        }
    }
}


/// Map for data stored on disk.
/// Rather have a second cluster type than inflating size
/// of output map by defining Option::None constantly.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct MapCluster {
    pub domain: Domain,
    pub status: Status,
}

impl Default for MapCluster {
    fn default() -> Self {
        MapCluster { domain: Domain::default(), status: Status::default() }
    }
}

impl From<Cluster> for MapCluster {
    fn from(cluster: Cluster) -> Self {
        MapCluster {
            domain: cluster.domain,
            status: cluster.status,
        }
    }
}


#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Status {
    Untested,
    ForIsolation(u8),
    Damaged,
}

impl Default for Status {
    fn default() -> Self {
        Status::Untested
    }
}


#[allow(unused)]
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
                status: Status::Untested,
            }],
        }
    }
}

#[allow(dead_code)]
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
    fn update(self, new_cluster: Cluster) {
        let mut new_map: Vec<MapCluster> = vec![MapCluster::from(new_cluster.to_owned())];

        for map_cluster in self.map.iter() {
            let mut map_cluster = *map_cluster;

            // If new_cluster doesn't start ahead and end short, map_cluster is forgotten.
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
                        status: map_cluster.status.to_owned()
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
    }

    /// Get current recovery status.
    pub fn get_state(self) -> Status {
        let mut recover_status = Status::Damaged;
        let mut cluster_stage: Option<u8> = Option::None;

        for cluster in self.map {
            match cluster.status {
                Status::Untested => return Status::Untested,
                Status::ForIsolation(cs) => {
                    if recover_status == Status::Damaged {
                        recover_status = cluster.status;
                    } else {
                        cluster_stage = Some(cs);
                    }
                },
                Status::Damaged => (),
            }

            if cluster_stage.is_some() {
                let recover_stage = match recover_status {
                    Status::ForIsolation(rs) => rs,
                    _ => unreachable!(),
                };

                if cluster_stage.unwrap() < recover_stage {
                    recover_status = cluster.status
                }

                cluster_stage = None
            }
        }

        recover_status
    }

    /// Get clusters of common status.
    pub fn get_clusters(self, state: Status) -> Vec<MapCluster> {
        self.map.iter()
            .filter_map(|mc| {
                if mc.status == state { Some(mc.to_owned()) } else { None }
            })
            .collect()
    }

    /// Defragments cluster groups.
    /// Algorithm could be improved to reduce extra looping.
    /// I.E. check forwards every cluster from current until status changes,
    /// then group at once.
    fn defrag(self) {
        let mut new_map: Vec<MapCluster> = vec![];
        let mut did_defrag = false;

        // Until completely defragged.
        while did_defrag {
            did_defrag = false;

            for current_cluster in self.map.iter() {
                // Find the trailing cluster of current.
                let trailing_cluster = self.map.iter()
                    .filter_map(|c| {
                        if c.domain.start == current_cluster.domain.end {
                            Some(c)
                        } else {
                            None
                        }
                    })
                    .nth(0);

                // If a cluster was found to be trailing 
                // (current cluster isn't the ending cluster)
                if trailing_cluster.is_some() {
                    let trailing_cluster = trailing_cluster.unwrap();

                    // Share common status; Defrag clusters.
                    if trailing_cluster.status == current_cluster.status {
                        // Create cluster encompassing both.
                        new_map.push(MapCluster {
                            domain: Domain {
                                start: current_cluster.domain.start,
                                end: trailing_cluster.domain.end,
                            },
                            status: current_cluster.status.to_owned(),
                        });
                        did_defrag = true;
                    } else {
                        // Otherwise, can't defrag this portion.
                        // Transfer current cluster to new_map
                        new_map.push(current_cluster.to_owned());
                    }
                }
            }
        }
    }

    fn defrag_new(self) {
        let mut new_map: Vec<MapCluster> = vec![];

        // ENSURE TO SORT OLD MAP IN ORDER OF SECTOR SEQUENCE

        let old_map = self.map.iter().enumerate();

        let mut pos: usize = 0;
        let end = old_map.last().unwrap().0;

        let new_map: Vec<MapCluster> = old_map
            .filter(|(index, cluster)| {
                if index < &pos {
                    return None
                }

                if old_map.nth(pos + 1).map(|(_, c)| c.status == cluster.status)? {

                }

                Some(**cluster)
            })
            .map(|(_, c)| *c)
            .collect();
    }
}