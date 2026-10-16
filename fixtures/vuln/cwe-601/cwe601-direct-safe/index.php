<?php

if (str_starts_with($_GET['x'], '/') && !str_starts_with($_GET['x'], '//')) {
    header('Location: ' . $_GET['x']);
}
