/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use webrender_traits::{DeviceIntRect, DeviceIntPoint, DeviceIntSize, DevicePixel};
use euclid::{TypedSize2D, TypedPoint2D, TypedRect, ScaleFactor, Length};

#[derive(Hash, Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct VirtualDevicePixel;
pub type VirtualIntSize = TypedSize2D<i32, VirtualDevicePixel>;
pub type VirtualIntPoint = TypedPoint2D<i32, VirtualDevicePixel>;
pub type VirtualIntRect = TypedRect<i32, VirtualDevicePixel>;

#[derive(Hash, Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct VirtualTile;
pub type VirtualTileIntSize = TypedSize2D<i32, VirtualTile>;
pub type VirtualTilePosition = TypedPoint2D<i32, VirtualTile>;
pub type VirtualTileRange = TypedRect<i32, VirtualTile>;

#[derive(Hash, Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DeviceTile;
pub type DeviceTileIntSize = TypedSize2D<i32, DeviceTile>;
pub type DeviceTileIntPoint = TypedPoint2D<i32, DeviceTile>;
pub type DeviceTileIntRect = TypedRect<i32, DeviceTile>;

type TileToVirtualDeviceScale = ScaleFactor<i32, VirtualTile, VirtualDevicePixel>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileId(u16);

impl TileId {
    fn index(self) -> usize { self.0 as usize }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TileState {
    NotAllocated,
    Allocated,
    Valid,
}

// Stored as a free list, with a twist: we preallocate all page structs and their
// offsets in the list is meaningful, so we can't use the structure in freelist.rs.
pub struct VirtualTexturePage {
    virtual_tile: Option<VirtualTilePosition>,
    tile_state: TileState,
}

pub struct VirtualTexture {
    table: VirtualPageTable,
    pages: Vec<VirtualTexturePage>,
    available_pages: Vec<TileId>,
    deallocated_pages: Vec<TileId>,
    device_tiles: DeviceTileIntSize,
    tile_size: i32,
    padding: i32,
}

pub struct VirtualPageTable {
    table: Vec<Option<TileId>>,
    size: VirtualTileIntSize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum DeallocOption {
    Reusable,
    Invalidate,
}

impl VirtualTexture {
    /// Constructs a virtual texture.
    pub fn new(virtual_size: VirtualIntSize,
               device_size: DeviceIntSize,
               tile_size: i32,
               padding: i32) -> Self {
        assert!(padding >= 0);
        assert!(tile_size > 0);
        assert!(tile_size - 2 * padding > 0);

        type TileToDeviceScale = ScaleFactor<i32, DeviceTile, DevicePixel>;
        let page_scale = TileToDeviceScale::new(tile_size);
        let tiles = DeviceTileIntSize::from_lengths(
            device_size.width_typed() / page_scale,
            device_size.height_typed() / page_scale,
        );

        let num_tiles = (tiles.width * tiles.height) as usize;
        let mut pages = Vec::with_capacity(num_tiles);
        let mut page_list = Vec::with_capacity(num_tiles);
        for i in 0..num_tiles {
            page_list.push(TileId(i as u16));
            pages.push(VirtualTexturePage{
                virtual_tile: None,
                tile_state: TileState::NotAllocated,
            });
        }

        VirtualTexture {
            table: VirtualPageTable::new(
                VirtualTexture::tiles_containing_size(virtual_size, tile_size, padding)
            ),
            pages: pages,
            available_pages: Vec::new(),
            deallocated_pages: page_list,
            tile_size: tile_size,
            padding: padding,
            device_tiles: tiles,
        }
    }

    /// Size of the tile in device pixels, minus the padding on each side.
    pub fn addressable_tile_size(&self) -> i32 {
        self.tile_size - 2 * self.padding
    }

    /// Size of the tile in device pixels, including the padding.
    pub fn allocated_tile_size(&self) -> i32 {
        self.tile_size
    }

    pub fn deallocate_tile(&mut self, tile: TileId, opt: DeallocOption) {
        debug_assert_eq!(self.get_tile_state(tile), TileState::Allocated);
        if opt == DeallocOption::Invalidate {
            let mut page = &mut self.pages[tile.index()];
            page.virtual_tile = None;
            page.tile_state = TileState::NotAllocated;
            self.deallocated_pages.push(tile);
        } else {
            self.pages[tile.index()].tile_state = TileState::Valid;
            self.available_pages.push(tile);
        }
    }

    fn device_position(&self, id: TileId) -> DeviceTileIntPoint {
        DeviceTileIntPoint::new(
            id.0 as i32 % self.device_tiles.width,
            id.0 as i32 / self.device_tiles.height,
        )
    }

    pub fn device_rect(&self, id: TileId) -> DeviceIntRect {
        let tile_pos = self.device_position(id);
        DeviceIntRect::new(
            DeviceIntPoint::new(tile_pos.x * self.tile_size, tile_pos.y * self.tile_size),
            DeviceIntSize::new(self.tile_size, self.tile_size),
        )
    }

    pub fn virtual_rect(&self, id: TileId) -> Option<VirtualIntRect> {
        self.virtual_tile(id).and_then(|position| {
            Some(self.virtual_rect_for_position(position))
        })
    }

    pub fn virtual_tile(&self, id: TileId) -> Option<VirtualTilePosition> {
        self.pages[id.index()].virtual_tile
    }

    pub fn virtual_to_device_position(&self, position: VirtualTilePosition) -> Option<DeviceTileIntPoint> {
        self.table.get_tile_id(position).map(|index| {
            self.device_position(index)
        })
    }

    pub fn device_rect_for_virtual_tile(&self, position: VirtualTilePosition) -> Option<DeviceIntRect> {
        self.table.get_tile_id(position).and_then(|id| {
            Some(self.device_rect(id))
        })
    }

    pub fn virtual_rect_for_position(&self, tile: VirtualTilePosition) -> VirtualIntRect {
        let inner_size = self.addressable_tile_size();
        let padded_size = self.allocated_tile_size();
        VirtualIntRect::new(
            VirtualIntPoint::new(tile.x * inner_size - self.padding,
                                 tile.y * inner_size - self.padding),
            VirtualIntSize::new(padded_size, padded_size),
        )
    }

    pub fn get_tile_state(&self, id: TileId) -> TileState {
        if id.0 as i32 > self.device_tiles.width * self.device_tiles.width {
            return TileState::NotAllocated;
        }

        self.pages[id.index()].tile_state
    }

    pub fn get_virtual_tile_state(&self, tile: VirtualTilePosition) -> TileState {
        if let Some(id) = self.table.get_tile_id(tile) {
            return self.get_tile_state(id);
        }
        return TileState::NotAllocated;
    }

    fn tiles_containing_size(size: VirtualIntSize,
                             tile_size: i32,
                             padding: i32) -> VirtualTileIntSize {
        let inner_tile_size = TileToVirtualDeviceScale::new(tile_size - 2 * padding);

        let mut w = size.width_typed() / inner_tile_size;
        let mut h = size.height_typed() / inner_tile_size;

        // Adjust the size because we need to round up, not down.
        if w.get() % inner_tile_size.get() > 0 {
            w += Length::new(1);
        }
        if h.get() % inner_tile_size.get() > 0 {
            h += Length::new(1);
        }

        VirtualTileIntSize::from_lengths(w, h)
    }

    /// Computes the range of virtual tiles that covers a rect.
    pub fn tiles_containing_rect(&self, rect: &VirtualIntRect) -> VirtualTileRange {
        let inner_tile_size = TileToVirtualDeviceScale::new(self.addressable_tile_size());

        let mut tile_rect = VirtualTileRange::new(
            VirtualTilePosition::from_lengths(
                rect.origin.x_typed() / inner_tile_size,
                rect.origin.x_typed() / inner_tile_size
            ),
            // The size is not correct yet since we snap the origin and we need to
            // round up the size, not down. This is adjusted below.
            VirtualTileIntSize::from_lengths(
                rect.size.width_typed() / inner_tile_size,
                rect.size.height_typed() / inner_tile_size
            ),
        );

        if tile_rect.max_x_typed() * inner_tile_size < rect.max_x_typed() {
            tile_rect.size.width += 1;
        }
        if tile_rect.max_y_typed() * inner_tile_size < rect.max_y_typed() {
            tile_rect.size.height += 1;
        }

        return tile_rect;
    }

    fn allocate_page(&mut self, virtual_tile: VirtualTilePosition) -> Option<TileId> {
        let alloc = self.deallocated_pages.pop().or_else(||{
            self.available_pages.pop()
        });

        if let Some(idx) = alloc {
            let page = &mut self.pages[idx.index()];
            page.virtual_tile = Some(virtual_tile);
            page.tile_state = TileState::Allocated;
        }

        return alloc;
    }

    /// Get a tile (allocating if needed) for a given tile position.
    pub fn get_or_allocate_tile(&mut self, tile: VirtualTilePosition) -> Option<TileId> {
        if let Some(existing) = self.table.get_tile_id(tile) {
            if self.pages[existing.index()].virtual_tile == Some(tile) {
                self.pages[existing.index()].tile_state = TileState::Allocated;
                return Some(existing)
            }
        }

        let alloc = self.allocate_page(tile);
        if alloc.is_some() {
            self.table.set_device_index(tile, alloc);
        }

        return alloc;
    }

    /// Allocate tiles for a given tile range (in tiles).
    ///
    /// Returns true if all tile allocations succeeded, false otherwise.
    pub fn allocate_tiles(&mut self, rect: &VirtualTileRange, pages: &mut Vec<TileAllocation>) -> bool {
        for y in rect.origin.y..rect.max_y() {
            for x in rect.origin.x..rect.max_y() {
                let tile = VirtualTilePosition::new(x, y);

                if let Some(alloc) = self.get_or_allocate_tile(tile) {
                    pages.push(TileAllocation {
                        id: alloc,
                        virtual_rect: self.virtual_rect_for_position(tile),
                    });
                } else {
                    return false;
                }
            }
        }
        return true;
    }

    /// Allocate enough tiles to cover the a given rect in virtual pixels.
    ///
    /// Returns true if all tile allocations succeeded, false otherwise.
    /// If a tile is already allocated for within the rect, it is reused.
    pub fn allocate_virtual_rect(&mut self, rect: &VirtualIntRect, pages: &mut Vec<TileAllocation>) -> bool {
        let tiles = self.tiles_containing_rect(rect);
        return self.allocate_tiles(&tiles, pages);
    }
}

#[derive(Debug)]
pub struct TileAllocation {
    pub id: TileId,
    pub virtual_rect: VirtualIntRect,
}

impl VirtualPageTable {
    pub fn new(size: VirtualTileIntSize) -> Self {
        let len = size.width as usize * size.height as usize;
        let mut table = Vec::with_capacity(len);

        for _ in 0..len {
            table.push(None);
        }

        return VirtualPageTable {
            table: table,
            size: size,
        };
    }

    pub fn get_tile_id(&self, pos: VirtualTilePosition) -> Option<TileId> {
        if pos.x < 0 || pos.y < 0 || pos.x >= self.size.width || pos.y >= self.size.height {
            return None;
        }

        let offset = (pos.x + self.size.width * pos.y) as usize;
        return self.table[offset];
    }

    pub fn set_device_index(&mut self, pos: VirtualTilePosition, index: Option<TileId>) {
        if pos.x < 0 || pos.y < 0 || pos.x >= self.size.width || pos.y >= self.size.height {
            panic!("Texture page is out of bounds");
        }

        let offset = (pos.x + self.size.width * pos.y) as usize;
        self.table[offset] = index;
    }
}

#[test]
fn simple_test() {
    use std::collections::HashSet;

    let tile_size = 128;
    let padding = 1;
    let mut vtex = VirtualTexture::new(
        VirtualIntSize::new(10000, 1000000),
        DeviceIntSize::new(1024, 1024),
        tile_size, padding,
    );

    let viewport = VirtualIntRect::new(
        VirtualIntPoint::new(15, 153),
        VirtualIntSize::new(800, 600),
    );

    let mut pages = Vec::new();
    let ok = vtex.allocate_virtual_rect(&viewport, &mut pages);
    assert!(ok);

    let mut set = HashSet::new();
    for alloc in &pages {
        // Check that the tile rect has the correct size.
        assert_eq!(alloc.virtual_rect.size, VirtualIntSize::new(tile_size, tile_size));
        // Check that the tile state is correct
        assert_eq!(vtex.get_tile_state(pages[0].id), TileState::Allocated);

        set.insert(alloc.id);
    }

    // Check that we don't have duplicates.
    assert_eq!(set.len(), pages.len());

    let tile = pages[0].id;
    let tile_pos = vtex.virtual_tile(tile).unwrap();

    assert_eq!(vtex.get_tile_state(tile), TileState::Allocated);

    // Request the same tile (should reuse the tile).
    vtex.get_or_allocate_tile(tile_pos);
    assert_eq!(vtex.get_tile_state(tile), TileState::Allocated);

    // Mark a tile as reusable for some later allocation without invalidating
    // its content.
    vtex.deallocate_tile(tile, DeallocOption::Reusable);
    assert_eq!(vtex.get_tile_state(tile), TileState::Valid);

    // Reallocate that tile (should reuse the tile).
    vtex.get_or_allocate_tile(tile_pos);
    assert_eq!(vtex.get_tile_state(tile), TileState::Allocated);

    for alloc in &pages {
        vtex.deallocate_tile(alloc.id, DeallocOption::Invalidate);
        assert_eq!(vtex.get_tile_state(alloc.id), TileState::NotAllocated);
    }
}
