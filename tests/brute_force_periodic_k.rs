use fnntw::{Tree, NotNan, distance::squared_euclidean};
use rand::{rngs::ThreadRng, Rng};



const NDATA: usize = 100;
const NQUERY: usize = 10_000;
const BOXSIZE: [NotNan<f64>; 3] = [ unsafe{ NotNan::new_unchecked(1.0) }; 3];
const D: usize = 3;
const K: usize = 4;

#[test]
fn test_brute_force_periodic_k() {

    // Random number generator
    let mut rng = rand::thread_rng();

    // Generate random data, query
    let mut data = Vec::with_capacity(NDATA);
    let mut query = Vec::with_capacity(NQUERY);
    for _ in 0..NDATA {
        data.push(random_point(&mut rng));
    }
    for _ in 0..NQUERY {
        query.push(random_point(&mut rng));
    }

    // Construct tree
    let tree = Tree::<'_, D>::new(&data, 32).unwrap();

    // Query tree
    let mut results = Vec::with_capacity(NQUERY);
    for q in &query {
        results.push(tree.query_nearest_k_periodic(q, K, &BOXSIZE));
    }

    // Brute force check results
    for (i, q) in query.iter().enumerate() {
        let result = &results[i];
        let expected = brute_force_periodic_k(q, &data, K);
        assert_eq!(result.len(), K);
        assert_eq!(expected.len(), K);
        assert_eq!(results[i], expected);
    }

}


fn random_point<const D: usize>(rng: &mut ThreadRng) -> [NotNan<f64>; D] {
    [(); D].map(|_| unsafe { std::mem::transmute::<f64, NotNan<f64>>(rng.gen()) })
}


fn brute_force_periodic_k<'d, const D: usize>(
    q: &[NotNan<f64>; D],
    data: &'d [[NotNan<f64>; D]],
    k: usize,
) -> Vec<(f64, u64, &'d[NotNan<f64>; D])> {

    // Costly...
    let mut all = Vec::with_capacity(data.len() * 2_usize.pow(D as u32));

    for (d, i) in data.iter().zip(0..) {
        
        // Note this is 0..2^D here so that we can check all images incl real without
        // checking for which we don't need to do (as in the library)
        for image in 0..2_usize.pow(D as u32) {
            
            // Closest image in the form of bool array
            let closest_image = (0..D)
                .map(|idx| ((image / 2_usize.pow(idx as u32)) % 2) == 1);

            
            let mut image_to_check = q.clone();
            
            for (idx, flag) in closest_image.enumerate() {

                // If moving image along this dimension
                if flag {

                    // Do a single index here. This is equal to distance to lower side
                    let query_component: &NotNan<f64> =  unsafe { q.get_unchecked(idx) };

                    // Single index here as well
                    let boxsize_component = unsafe { BOXSIZE.get_unchecked(idx) };

                    unsafe {
                        if *query_component < boxsize_component / 2.0 {
                            // Add if in lower half of box
                            *image_to_check.get_unchecked_mut(idx) = query_component + boxsize_component
                        } else {
                            // Subtract if in upper half of box
                            *image_to_check.get_unchecked_mut(idx) = query_component - boxsize_component
                        }
                    }
                    
                }
            }

            let dist = squared_euclidean(&image_to_check, d);

            all.push((dist, i, d))
        }
    }

    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    all.truncate(k);
    all
}