use std::{ env, fs};
use anyhow::Result;
pub fn init() -> Result<()> {
    
    let curr_dir = env::current_dir()?;
    let rusty_dir = curr_dir.join(".rusty");
    
    if rusty_dir.exists(){
        println!("Repo already exists");
        return Ok(())
    }

    fs::create_dir_all(rusty_dir.join("objects/blobs"))?;
    fs::create_dir_all(rusty_dir.join("objects/trees"))?;
    fs::create_dir_all(rusty_dir.join("objects/commits"))?;

    fs::create_dir_all(rusty_dir.join("refs/heads"))?;
    fs::create_dir_all(rusty_dir.join("refs/remotes"))?;

    fs::write(rusty_dir.join("HEAD"), "ref: refs/heads/main\n",)?;
    fs::write(rusty_dir.join("index"), "")?;
    fs::write(rusty_dir.join("config"), "[core]\nrepositoryformatversion = 1\n")?;

    println!("Empty repository initialsied");

    Ok(())
}
