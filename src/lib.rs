use ordered_float::NotNan;

#[cfg(feature = "timing")]
use std::sync::atomic::Ordering;

#[cfg(feature = "timing")]
use num_format::{Locale, ToFormattedString};



// mod medians;
#[cfg(feature = "timing")]
static LEAF_VEC_ALLOC: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "timing")]
static LEAF_WRITE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "timing")]
static STEM_MEDIAN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "timing")]
static STEM_WRITE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "timing")]
static mut TOTAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub struct Tree<'t, const D: usize> {
    data: &'t [[NotNan<f64>; D]],
    leafsize: usize,
    nodes: Vec<Node<'t, D>>,
}

#[derive(Debug)]
enum Node<'t, const D: usize> {
    Stem {
        split_dim: usize,
        point: &'t [NotNan<f64>; D],
        left: usize,
        right: usize,
    },
    Leaf { 
        points: Leaf<'t, D>,
        lower: [NotNan<f64>; D],
        upper: [NotNan<f64>; D],        
    },
}

impl<'t, const D: usize> Node<'t, D> {

    fn is_stem(&self) -> bool {
        match self {
            Node::Stem { .. } => true,
            Node::Leaf { .. } => false,
        }
    }
    
    #[cfg(not(feature = "single_ref"))]
    /// This was here just to test performance vs single ref.
    /// double ref seems to win (this one is double ref)
    fn iter(&'t self) -> impl Iterator<Item=&'t &'t [NotNan<f64>; D]> {
        match self {
            Node::Leaf { points, .. } => points.iter(),
            _ => unreachable!("this function should only be used on leaves"),
        }
    }

    #[cfg(feature = "single_ref")]
    /// This was here just to test performance vs double ref.
    /// double ref seems to win
    fn iter(&'t self) -> impl Iterator<Item=&'t [NotNan<f64>; D]> {
        match self {
            Node::Leaf { points, .. } => points.clone().into_iter(),
            _ => unreachable!("this function should only be used on leaves"),
        }
    }

}


type Leaf<'t, const D: usize> = Vec<&'t [NotNan<f64>; D]>;

impl<'t, const D: usize> Tree<'t, D> {
    
    pub fn new(
        data: &'t [[NotNan<f64>; D]],
        leafsize: usize,
    ) -> Result<Tree<'t, D>, &'static str> {
        
        // Nonzero Length
        let data_len =  data.len();
        if data_len == 0 {
            return Err("data has zero length")
        }

        // Unsafe operations require leafsize to be at least 4
        // Also probably a good idea to keep above 4 anyway.
        if leafsize < 4 {
            return Err("Choose a leafsize >= 4")
        }

        // Initialize variables for recursive function
        let split_level: usize = 0;
        #[cfg(feature = "timing")]
        let timer = std::time::Instant::now();
        let vec_ref: &mut [&'t [NotNan<f64>; D]] = &mut data.iter().collect::<Vec<_>>();
        #[cfg(feature = "timing")]
        let initial_vec_ref = timer.elapsed().as_nanos();
        let mut nodes = vec![];

        // Run recursive build
        Tree::<'t, D>::build_nodes(vec_ref, split_level, leafsize, &mut nodes);

        #[cfg(feature = "timing")]
        {
            // safe because no other thread can hold this mutable reference
            unsafe { 
                *TOTAL.get_mut() = initial_vec_ref as usize
                    + LEAF_VEC_ALLOC.load(Ordering::SeqCst)
                    + LEAF_WRITE.load(Ordering::SeqCst)
                    + STEM_MEDIAN.load(Ordering::SeqCst)
                    + STEM_WRITE.load(Ordering::SeqCst);
            }
            
            // Load atomics
            let total = unsafe { TOTAL.load(Ordering::SeqCst) };
            let leaf_write = LEAF_WRITE.load(Ordering::SeqCst);
            let leaf_vec_alloc = LEAF_VEC_ALLOC.load(Ordering::SeqCst);
            let stem_median = STEM_MEDIAN.load(Ordering::SeqCst);
            let stem_write = STEM_WRITE.load(Ordering::SeqCst);
            
            // Time elapsed strs
            let total_str = total.to_formatted_string(&Locale::en);
            let ivr_str = initial_vec_ref.to_formatted_string(&Locale::en);
            let leaf_write_str = leaf_write.to_formatted_string(&Locale::en);
            let leaf_vec_alloc_str = leaf_vec_alloc.to_formatted_string(&Locale::en);
            let stem_median_str = stem_median.to_formatted_string(&Locale::en);
            let stem_write_str = stem_write.to_formatted_string(&Locale::en);

            // Frac strs
            let ivr_frac_str = format!("{:.2}", 100.0 * initial_vec_ref as f64 / total as f64);
            let leaf_write_frac_str = format!("{:.2}", 100.0 * leaf_write as f64 / total as f64);
            let leaf_vec_alloc_frac_str = format!("{:.2}", 100.0 * leaf_vec_alloc as f64 / total as f64);
            let stem_median_frac_str = format!("{:.2}", 100.0 * stem_median as f64 / total as f64);
            let stem_write_frac_str = format!("{:.2}", 100.0 * stem_write as f64 / total as f64);

            println!("\nINITIAL_VEC_REF = {} nanos, {}%", ivr_str, ivr_frac_str);
            println!("LEAF_VEC_ALLOC = {} nanos, {}%", leaf_vec_alloc_str, leaf_vec_alloc_frac_str);
            println!("LEAF_WRITE = {} nanos {}%", leaf_write_str, leaf_write_frac_str);
            println!("STEM_MEDIAN = {} nanos, {}%", stem_median_str, stem_median_frac_str);
            println!("STEM_WRITE = {} nanos, {}%", stem_write_str, stem_write_frac_str);
            println!("TOTAL = {}\n", total_str);
        }

        Ok(Tree {
            data,
            leafsize,
            nodes,
        })
    }


    // A recursive private function.
    fn build_nodes<'a>(
        subset: &'a mut[&'t [NotNan<f64>; D]],
        mut split_level: usize,
        leafsize: usize,
        nodes: &mut Vec<Node<'t, D>>,
    ) -> usize {

        // Increment split level
        split_level += 1;

        // Get split dimension
        let split_dim = split_level % D;

        // Determine leaf-ness
        let is_leaf =  subset.len() <= leafsize;
        
        match is_leaf {
            true => {

                #[cfg(feature = "timing")]
                let timer = std::time::Instant::now();
                let mut lower = [(); D].map(|_| unsafe { NotNan::new_unchecked(std::f64::MAX) });
                let mut upper = [(); D].map(|_| unsafe { NotNan::new_unchecked(std::f64::MIN) });
                for i in 0..D {
                    for p in 0..subset.len() {
                        unsafe {

                            // get mut refs
                            let lower_i = lower.get_unchecked_mut(i);
                            let upper_i = upper.get_unchecked_mut(i);
                            *lower_i = *(&*lower_i).min(subset.get_unchecked(p).get_unchecked(i));
                            *upper_i = *(&*upper_i).min(subset.get_unchecked(p).get_unchecked(i));
                        }
                    }
                }
                let leaf = Node::Leaf {
                    points: subset.to_vec(),
                    lower,
                    upper,
                };
                #[cfg(feature = "timing")]
                let vec_alloc = timer.elapsed().as_nanos();
                #[cfg(feature = "timing")]
                LEAF_VEC_ALLOC.fetch_add(vec_alloc as usize, Ordering::SeqCst);

                #[cfg(feature = "timing")]
                let timer = std::time::Instant::now();
                let leaf_index = nodes.len();
                nodes.push(leaf);
                #[cfg(feature = "timing")]
                let write_time = timer.elapsed().as_nanos();
                #[cfg(feature = "timing")]
                LEAF_WRITE.fetch_add(write_time as usize, Ordering::SeqCst);

                leaf_index
            },
            false => {
                
                // Calculate index of median
                let median_index = subset.len() / 2;

                #[cfg(feature = "timing")]
                let timer = std::time::Instant::now();
                // Select median in this subset based on split_dim component
                let (left, median, right) = subset.select_nth_unstable_by(median_index, |a, b| { 
                    unsafe { a.get_unchecked(split_dim).cmp(&b.get_unchecked(split_dim)) }
                });
                #[cfg(feature = "timing")]
                let stem_median = timer.elapsed().as_nanos();
                #[cfg(feature = "timing")]
                STEM_MEDIAN.fetch_add(stem_median as usize, Ordering::SeqCst);


                let left_handle = Tree::build_nodes(left, split_level, leafsize, nodes);
                let right_handle = Tree::build_nodes(right, split_level, leafsize, nodes);


                let stem = Node::Stem {
                    split_dim,
                    point: median,
                    left: left_handle,
                    right: right_handle,
                };

                #[cfg(feature = "timing")]
                let timer = std::time::Instant::now();
                let stem_index = nodes.len();
                nodes.push(stem);
                #[cfg(feature = "timing")]
                let stem_write = timer.elapsed().as_nanos();
                #[cfg(feature = "timing")]
                STEM_WRITE.fetch_add(stem_write as usize, Ordering::SeqCst);

                stem_index
            }
        }
    }

    pub fn size(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {

    use crate::Tree;
    use ordered_float::NotNan;
    use concat_idents::concat_idents;
    use seq_macro::seq;

    // Generate 1..16 dimensional size=1 kd tree unit tests
    macro_rules! size_one_kdtree {
        ($d:ident) => {
            concat_idents!(test_name = test_make_, $d, _, dtree, {
                #[test]
                fn test_name() {
            
                    let leafsize = 16; 

                    let data: Vec<_> = (0..leafsize).map(|x| {
                        [NotNan::new(x as f64).unwrap(); $d]
                    }).collect();
            
            
                    let tree = Tree::new(&data, leafsize).unwrap();
                    assert_eq!(tree.size(), 1);
                }
            });
        };
    }
    seq!(D in 0..=8 {
        #[allow(non_upper_case_globals)]
        const two_pow~D: usize = 2_usize.pow(D);
        size_one_kdtree!(two_pow~D);
    });


    #[test]
    fn test_make_1dtree_with_size_three() {

        let data: Vec<[NotNan<f64>; 1]> = [
            [(); 32].map(|_| unsafe { [NotNan::new_unchecked(0.1)] }).as_ref(),
            [(); 1].map(|_| unsafe { [NotNan::new_unchecked(0.5)] }).as_ref(),
            [(); 32].map(|_| unsafe { [NotNan::new_unchecked(0.9)] }).as_ref(),
        ].concat();

        let leafsize = 32;

        let tree = Tree::new(&data, leafsize).unwrap();
        for node in &tree.nodes {
            println!("{node:?}")
        }
        assert_eq!(tree.size(), 3);
    }
}

