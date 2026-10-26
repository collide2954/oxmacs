use rodeo::random::Random;

#[test]
fn seeds_expand_through_splitmix64() {
    assert_eq!(
        Random::new(0).state(),
        [
            0xe220_a839_7b1d_cdaf,
            0x6e78_9e6a_a1b9_65f4,
            0x06c4_5d18_8009_454f,
            0xf88b_b8a8_724c_81ec
        ]
    );
}

#[test]
fn the_stream_is_xoshiro256_star_star() {
    let mut random = Random::from_state([1, 2, 3, 4]);
    assert_eq!(
        [random.next_u64(), random.next_u64(), random.next_u64()],
        [11520, 0, 1_509_978_240]
    );
}

#[test]
fn the_same_seed_gives_the_same_numbers() {
    let mut a = Random::new(42);
    let mut b = Random::new(42);
    let first: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
    let second: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
    assert_eq!(first, second);
    assert_ne!(Random::new(43).next_u64(), first[0]);
    let mut resumed = Random::from_state(a.state());
    assert_eq!(resumed.next_u64(), b.next_u64());
}

#[test]
fn draws_stay_in_their_ranges() {
    let mut random = Random::new(7);
    for _ in 0..1000 {
        let fraction = random.float();
        assert!((0.0..1.0).contains(&fraction));
        assert!((-3..5).contains(&random.int(-3, 5)));
        assert!(random.below(6) < 6);
        assert!((2.0..4.0).contains(&random.range(2.0, 4.0)));
    }
    assert!(!random.chance(0.0));
    assert!(random.chance(1.0));
    assert_eq!(random.int(5, 5), 5);
}

#[test]
fn picks_and_shuffles_use_every_item() {
    let mut random = Random::new(9);
    let items = [1, 2, 3, 4, 5];
    assert!(items.contains(random.pick(&items).unwrap()));
    assert_eq!(random.pick::<i32>(&[]), None);
    let mut shuffled = items;
    random.shuffle(&mut shuffled);
    let mut sorted = shuffled;
    sorted.sort_unstable();
    assert_eq!(sorted, items);
}
